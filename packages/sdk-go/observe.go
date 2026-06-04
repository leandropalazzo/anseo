// Package observe is a thin instrumentation SDK for the OpenGEO
// Run-Ingestion API.
//
// The OpenTelemetry pattern, minus the ceremony: you already ran a prompt
// against an LLM provider outside OpenGEO. This SDK lets you POST that run to
// POST /v1/ingest/run in one call, so it flows through the same
// extraction -> redaction -> benchmark-contribution path as a native run.
//
// Only the standard library is used.
//
//	observer, _ := observe.New(observe.Config{
//	    BaseURL: "https://opengeo.internal",
//	    APIKey:  os.Getenv("OPENGEO_API_KEY"),
//	    Project: "Sunski",
//	})
//
//	res, err := observer.ObserveRun(ctx, observe.RunInput{
//	    PromptSlug:   "best-polarized-sunglasses",
//	    Provider:     "openai",
//	    Model:        "gpt-4o-2024-08-06",
//	    ResponseText: completionText,
//	})
package observe

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

const ingestPath = "/v1/ingest/run"

// Int returns a pointer to v, for setting optional pointer fields like
// RunInput.ObservedRank inline.
func Int(v int) *int { return &v }

// Config configures an Observer.
type Config struct {
	// BaseURL of the OpenGEO API, e.g. "https://opengeo.internal".
	BaseURL string
	// APIKey is sent as the X-OpenGEO-API-Key header.
	APIKey string
	// Project scopes the run, sent as the X-OpenGEO-Project header (resolved by
	// brand name server-side). Optional for single-project deployments.
	Project string
	// HTTPClient lets callers inject a custom client (timeouts, transport,
	// test server). Defaults to a client with a 30s timeout.
	HTTPClient *http.Client
}

// RunInput is one externally-executed run to record. Mirrors the API's
// IngestRunRequest.
type RunInput struct {
	// PromptSlug is the declared prompt slug within the project. Must already
	// exist server-side.
	PromptSlug string
	// Provider that produced the run, e.g. "openai".
	Provider string
	// Model version, e.g. "gpt-4o-2024-08-06".
	Model string
	// ResponseText is the raw response the provider returned. Optional when
	// CitationDomains is supplied.
	ResponseText string
	// CitationDomains observed in the run's citations. When nil and
	// ResponseText is set, the server extracts domains from the text.
	CitationDomains []string
	// ObservedRank is the brand's observed rank, if computed. Use nil to omit.
	ObservedRank *int
	// ObservedAt is when the run was observed. Zero value omits it (server
	// defaults to now).
	ObservedAt time.Time
}

// ContributionStatus mirrors the API's internally-tagged ContributionStatus
// enum: Status is one of "sealed", "skipped_not_opted_in", "kek_missing", or
// "redaction_rejected" (in which case Reason is populated).
type ContributionStatus struct {
	Status string `json:"status"`
	Reason string `json:"reason,omitempty"`
}

// RunResult is the parsed IngestRunResponse.
type RunResult struct {
	RunID        string             `json:"run_id"`
	ProjectID    string             `json:"project_id"`
	PromptSlug   string             `json:"prompt_slug"`
	Provider     string             `json:"provider"`
	ObservedAt   string             `json:"observed_at"`
	Contribution ContributionStatus `json:"contribution"`
}

// APIError is returned when the API responds with a non-2xx status.
type APIError struct {
	// Status is the HTTP status code.
	Status int
	// Code is the machine-readable error code from the API body, if present.
	Code string
	// Message is a human-readable message.
	Message string
}

func (e *APIError) Error() string {
	if e.Code != "" {
		return fmt.Sprintf("opengeo ingest failed: HTTP %d (%s): %s", e.Status, e.Code, e.Message)
	}
	return fmt.Sprintf("opengeo ingest failed: HTTP %d: %s", e.Status, e.Message)
}

// Observer is a thin client around POST /v1/ingest/run.
type Observer struct {
	baseURL string
	apiKey  string
	project string
	client  *http.Client
}

// New constructs an Observer. BaseURL and APIKey are required.
func New(cfg Config) (*Observer, error) {
	if cfg.BaseURL == "" {
		return nil, errors.New("observe: BaseURL is required")
	}
	if cfg.APIKey == "" {
		return nil, errors.New("observe: APIKey is required")
	}
	client := cfg.HTTPClient
	if client == nil {
		client = &http.Client{Timeout: 30 * time.Second}
	}
	return &Observer{
		// Normalize trailing slashes so URL joining is unambiguous.
		baseURL: strings.TrimRight(cfg.BaseURL, "/"),
		apiKey:  cfg.APIKey,
		project: cfg.Project,
		client:  client,
	}, nil
}

// wireRequest is the snake_case body the API expects. Pointers/omitempty let
// unset optional fields be omitted so server-side defaults apply.
type wireRequest struct {
	PromptSlug      string   `json:"prompt_slug"`
	Provider        string   `json:"provider"`
	Model           string   `json:"model"`
	ResponseText    string   `json:"response_text,omitempty"`
	CitationDomains []string `json:"citation_domains,omitempty"`
	ObservedRank    *int     `json:"observed_rank,omitempty"`
	ObservedAt      string   `json:"observed_at,omitempty"`
}

// ObserveRun records one externally-executed run. It returns the parsed
// RunResult, or an *APIError on a non-2xx response.
func (o *Observer) ObserveRun(ctx context.Context, input RunInput) (*RunResult, error) {
	body := wireRequest{
		PromptSlug:      input.PromptSlug,
		Provider:        input.Provider,
		Model:           input.Model,
		ResponseText:    input.ResponseText,
		CitationDomains: input.CitationDomains,
		ObservedRank:    input.ObservedRank,
	}
	if !input.ObservedAt.IsZero() {
		body.ObservedAt = input.ObservedAt.UTC().Format(time.RFC3339Nano)
	}

	encoded, err := json.Marshal(body)
	if err != nil {
		return nil, fmt.Errorf("observe: encode request: %w", err)
	}

	req, err := http.NewRequestWithContext(
		ctx, http.MethodPost, o.baseURL+ingestPath, bytes.NewReader(encoded),
	)
	if err != nil {
		return nil, fmt.Errorf("observe: build request: %w", err)
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-OpenGEO-API-Key", o.apiKey)
	if o.project != "" {
		req.Header.Set("X-OpenGEO-Project", o.project)
	}

	resp, err := o.client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("observe: send request: %w", err)
	}
	defer resp.Body.Close()

	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("observe: read response: %w", err)
	}

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		var errBody struct {
			Error   string `json:"error"`
			Message string `json:"message"`
		}
		_ = json.Unmarshal(raw, &errBody)
		message := errBody.Message
		if message == "" {
			message = fmt.Sprintf("HTTP %d", resp.StatusCode)
		}
		return nil, &APIError{Status: resp.StatusCode, Code: errBody.Error, Message: message}
	}

	var result RunResult
	if err := json.Unmarshal(raw, &result); err != nil {
		return nil, fmt.Errorf("observe: decode response: %w", err)
	}
	return &result, nil
}
