// Package observe is a thin instrumentation SDK for the Anseo Run-Ingestion
// API (Story 40.3).
//
// The OpenTelemetry pattern, minus the ceremony: you already ran a prompt
// against an LLM provider outside Anseo. This SDK lets you POST that run to
// POST /v1/ingest/run in one call, so it flows through the same
// extraction -> redaction -> benchmark-contribution path as a native run.
//
// It is the Go port of the Python reference (packages/sdk-python/anseo_observe),
// implementing the same language-agnostic spec in docs/sdk-spec.md. Only the
// standard library is used.
//
// Two delivery surfaces, by design:
//
//   - ObserveRun — strict. Returns the parsed RunResult and an *APIError on a
//     non-2xx response. For manual, synchronous control (e.g. a backfill that
//     reads each contribution status).
//   - Send and the Observe helper — best-effort, at-most-once. Observability
//     must never interrupt the host app, so any transport/server error is
//     logged (at DEBUG, or WARN for a 401) and swallowed. No status is ever
//     retried (a retry on 5xx could double-record a run the server already
//     processed before timing out).
//
//	observer, _ := observe.New(observe.Config{
//	    BaseURL: "https://anseo.internal",
//	    APIKey:  os.Getenv("ANSEO_API_KEY"),
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
	"log"
	"net/http"
	"os"
	"strings"
	"time"
)

const ingestPath = "/v1/ingest/run"

// Canonical auth + project headers (post-rename). The API also accepts the
// legacy X-OpenGEO-* spellings, but new clients send the canonical names.
const (
	apiKeyHeader  = "X-Anseo-API-Key"
	projectHeader = "X-Anseo-Project"
)

// Int returns a pointer to v, for setting optional pointer fields like
// RunInput.ObservedRank inline.
func Int(v int) *int { return &v }

// Bool returns a pointer to v, for setting optional pointer fields like
// RunInput.Contribute inline.
func Bool(v bool) *bool { return &v }

// Logger is the minimal diagnostics sink the SDK writes to. The standard
// library *log.Logger satisfies it.
type Logger interface {
	Printf(format string, v ...any)
}

// ConfigError is returned by New for an invalid configuration (missing BaseURL
// or APIKey). A misconfigured client is a programming error the developer must
// fix; New reports it eagerly rather than deferring it to a call. Mirrors the
// Python reference's AnseoConfigError.
type ConfigError struct{ Msg string }

func (e *ConfigError) Error() string { return "observe: " + e.Msg }

// Config configures an Observer.
type Config struct {
	// BaseURL of the Anseo API, e.g. "https://anseo.internal". Required.
	BaseURL string
	// APIKey is sent as the X-Anseo-API-Key header. Required. Sole auth.
	APIKey string
	// Project scopes the run, sent as the X-Anseo-Project header (resolved by
	// brand name server-side). Optional for single-project deployments.
	Project string
	// HTTPClient lets callers inject a custom client (timeouts, transport,
	// test server). Defaults to a client with a 30s timeout.
	HTTPClient *http.Client
	// Logger receives best-effort diagnostics. Defaults to a logger that emits
	// WARN-level lines to stderr and DEBUG lines only when DEBUG names "anseo".
	Logger Logger
}

// RunInput is one externally-executed run to record. Mirrors the API's
// IngestRunRequest.
type RunInput struct {
	// PromptSlug is the declared prompt slug within the project. Must already
	// exist server-side.
	PromptSlug string
	// Provider that produced the run, e.g. "openai"; "unknown" if undetectable.
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
	// Contribute opts this run into Anseo's benchmark contribution path. Nil
	// omits the field, preserving the server default (`false`). Use Bool(true)
	// for explicit per-run contribution.
	Contribute *bool
}

// ContributionStatus mirrors the API's internally-tagged ContributionStatus
// enum: Status is typically "sealed", "skipped_not_opted_in", or
// "redaction_rejected" (in which case Reason is populated). The
// "kek_missing" value is retained for wire compatibility even though current
// servers reject `contribute: true` without a KEK as HTTP 403 before
// persistence.
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

// APIError is returned by the strict ObserveRun when the API responds with a
// non-2xx status.
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
		return fmt.Sprintf("anseo ingest failed: HTTP %d (%s): %s", e.Status, e.Code, e.Message)
	}
	return fmt.Sprintf("anseo ingest failed: HTTP %d: %s", e.Status, e.Message)
}

// Observer is a thin client around POST /v1/ingest/run.
type Observer struct {
	baseURL string
	apiKey  string
	project string
	client  *http.Client
	logger  Logger
}

// New constructs an Observer. BaseURL and APIKey are required; a missing one is
// reported as a *ConfigError.
func New(cfg Config) (*Observer, error) {
	if cfg.BaseURL == "" {
		return nil, &ConfigError{Msg: "BaseURL is required"}
	}
	if cfg.APIKey == "" {
		return nil, &ConfigError{Msg: "APIKey is required"}
	}
	client := cfg.HTTPClient
	if client == nil {
		client = &http.Client{Timeout: 30 * time.Second}
	}
	logger := cfg.Logger
	if logger == nil {
		logger = defaultLogger()
	}
	return &Observer{
		// Normalize trailing slashes so URL joining is unambiguous.
		baseURL: strings.TrimRight(cfg.BaseURL, "/"),
		apiKey:  cfg.APIKey,
		project: cfg.Project,
		client:  client,
		logger:  logger,
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
	Contribute      *bool    `json:"contribute,omitempty"`
}

// ObserveRun records one externally-executed run (strict). It returns the
// parsed RunResult, or an *APIError on a non-2xx response (and a wrapped error
// on a transport/encode/decode failure).
func (o *Observer) ObserveRun(ctx context.Context, input RunInput) (*RunResult, error) {
	body := wireRequest{
		PromptSlug:      input.PromptSlug,
		Provider:        input.Provider,
		Model:           input.Model,
		ResponseText:    input.ResponseText,
		CitationDomains: input.CitationDomains,
		ObservedRank:    input.ObservedRank,
		Contribute:      input.Contribute,
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
	req.Header.Set(apiKeyHeader, o.apiKey)
	if o.project != "" {
		req.Header.Set(projectHeader, o.project)
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

// Send is the best-effort, at-most-once surface. It never returns an error and
// never retries: per the core spec, observability must not interrupt the host
// app. It returns the RunResult on success, or nil when the run could not be
// delivered.
//
//   - transport/timeout/decode errors are logged at DEBUG and discarded;
//   - a 401 (bad API key) is logged at WARN so the operator notices, but is
//     still swallowed;
//   - no status is ever retried (at-most-once delivery).
//
// Enable DEBUG diagnostics with the DEBUG=anseo environment variable.
func (o *Observer) Send(ctx context.Context, input RunInput) *RunResult {
	res, err := o.ObserveRun(ctx, input)
	if err == nil {
		return res
	}
	var apiErr *APIError
	if errors.As(err, &apiErr) {
		if apiErr.Status == http.StatusUnauthorized {
			o.logger.Printf("anseo: WARN ingest rejected (401) — check your API key; this run was NOT recorded: %s", apiErr.Message)
		} else {
			debugf(o.logger, "ingest returned HTTP %d (%s); run discarded: %s", apiErr.Status, apiErr.Code, apiErr.Message)
		}
		return nil
	}
	// network/timeout/encode/decode — best-effort: log and discard.
	debugf(o.logger, "ingest send failed; run discarded: %v", err)
	return nil
}

// defaultLogger returns a stderr logger. DEBUG-level lines are gated by debugf;
// WARN lines always print.
func defaultLogger() Logger {
	return log.New(os.Stderr, "", log.LstdFlags)
}

// debugEnabled reports whether DEBUG diagnostics should be emitted, honouring
// the DEBUG=anseo convention shared with the Python/TS SDKs.
func debugEnabled() bool {
	flag := os.Getenv("DEBUG")
	if flag == "" {
		return false
	}
	if flag == "*" || flag == "1" || flag == "true" {
		return true
	}
	for _, tok := range strings.FieldsFunc(flag, func(r rune) bool {
		return r == ',' || r == ' '
	}) {
		if tok == "anseo" || tok == "anseo:*" {
			return true
		}
	}
	return false
}

func debugf(l Logger, format string, v ...any) {
	if debugEnabled() {
		l.Printf("anseo: DEBUG "+format, v...)
	}
}
