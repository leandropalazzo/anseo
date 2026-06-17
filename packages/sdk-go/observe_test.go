package observe

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

const okBody = `{
  "run_id": "run_123",
  "project_id": "proj_abc",
  "prompt_slug": "best-polarized-sunglasses",
  "provider": "openai",
  "observed_at": "2026-06-04T12:00:00Z",
  "contribution": {"status": "sealed"}
}`

func intPtr(v int) *int { return &v }

func TestObserveRun_PostsHeadersAndSnakeCaseBody(t *testing.T) {
	var gotPath, gotMethod, gotAPIKey, gotProject, gotContentType string
	var gotBody map[string]any

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotPath = r.URL.Path
		gotMethod = r.Method
		gotAPIKey = r.Header.Get("X-Anseo-API-Key")
		gotProject = r.Header.Get("X-Anseo-Project")
		gotContentType = r.Header.Get("Content-Type")
		raw, _ := io.ReadAll(r.Body)
		_ = json.Unmarshal(raw, &gotBody)
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, okBody)
	}))
	defer srv.Close()

	// Trailing slash must be normalized, not doubled.
	observer, err := New(Config{BaseURL: srv.URL + "/", APIKey: "key-xyz", Project: "Sunski"})
	if err != nil {
		t.Fatalf("New: %v", err)
	}

	res, err := observer.ObserveRun(context.Background(), RunInput{
		PromptSlug:   "best-polarized-sunglasses",
		Provider:     "openai",
		Model:        "gpt-4o-2024-08-06",
		ResponseText: "Try Sunski, see https://sunski.com",
		ObservedRank: intPtr(1),
		ObservedAt:   time.Date(2026, 6, 4, 12, 0, 0, 0, time.UTC),
		Contribute:   Bool(true),
	})
	if err != nil {
		t.Fatalf("ObserveRun: %v", err)
	}

	if gotPath != "/v1/ingest/run" {
		t.Errorf("path = %q, want /v1/ingest/run", gotPath)
	}
	if gotMethod != http.MethodPost {
		t.Errorf("method = %q, want POST", gotMethod)
	}
	if gotAPIKey != "key-xyz" {
		t.Errorf("api key header = %q", gotAPIKey)
	}
	if gotProject != "Sunski" {
		t.Errorf("project header = %q", gotProject)
	}
	if gotContentType != "application/json" {
		t.Errorf("content-type = %q", gotContentType)
	}
	want := map[string]any{
		"prompt_slug":   "best-polarized-sunglasses",
		"provider":      "openai",
		"model":         "gpt-4o-2024-08-06",
		"response_text": "Try Sunski, see https://sunski.com",
		"observed_rank": float64(1),
		"observed_at":   "2026-06-04T12:00:00Z",
		"contribute":    true,
	}
	for k, v := range want {
		if gotBody[k] != v {
			t.Errorf("body[%q] = %v, want %v", k, gotBody[k], v)
		}
	}
	if len(gotBody) != len(want) {
		t.Errorf("body has %d keys, want %d: %v", len(gotBody), len(want), gotBody)
	}

	if res.RunID != "run_123" {
		t.Errorf("RunID = %q", res.RunID)
	}
	if res.Contribution.Status != "sealed" {
		t.Errorf("Contribution.Status = %q", res.Contribution.Status)
	}
}

func TestObserveRun_OmitsProjectHeaderAndOptionalFields(t *testing.T) {
	var hasProject bool
	var gotBody map[string]any

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, hasProject = r.Header["X-Anseo-Project"]
		raw, _ := io.ReadAll(r.Body)
		_ = json.Unmarshal(raw, &gotBody)
		_, _ = io.WriteString(w, okBody)
	}))
	defer srv.Close()

	observer, _ := New(Config{BaseURL: srv.URL, APIKey: "k"})
	if _, err := observer.ObserveRun(context.Background(), RunInput{
		PromptSlug: "p", Provider: "openai", Model: "m",
	}); err != nil {
		t.Fatalf("ObserveRun: %v", err)
	}

	if hasProject {
		t.Error("X-Anseo-Project header should be absent")
	}
	if len(gotBody) != 3 {
		t.Errorf("body should have exactly 3 keys, got %v", gotBody)
	}
}

func TestObserveRun_SurfacesKekMissing(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = io.WriteString(w, `{"run_id":"r","project_id":"p","prompt_slug":"s","provider":"openai","observed_at":"t","contribution":{"status":"kek_missing"}}`)
	}))
	defer srv.Close()

	observer, _ := New(Config{BaseURL: srv.URL, APIKey: "k"})
	res, err := observer.ObserveRun(context.Background(), RunInput{PromptSlug: "p", Provider: "openai", Model: "m"})
	if err != nil {
		t.Fatalf("ObserveRun: %v", err)
	}
	if res.Contribution.Status != "kek_missing" {
		t.Errorf("Contribution.Status = %q, want kek_missing", res.Contribution.Status)
	}
}

func TestObserveRun_ReturnsAPIErrorOnNon2xx(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNotFound)
		_, _ = io.WriteString(w, `{"error":"prompt_not_found","message":"prompt is not declared"}`)
	}))
	defer srv.Close()

	observer, _ := New(Config{BaseURL: srv.URL, APIKey: "k"})
	_, err := observer.ObserveRun(context.Background(), RunInput{PromptSlug: "p", Provider: "openai", Model: "m"})
	if err == nil {
		t.Fatal("expected an error")
	}
	apiErr, ok := err.(*APIError)
	if !ok {
		t.Fatalf("error type = %T, want *APIError", err)
	}
	if apiErr.Status != http.StatusNotFound {
		t.Errorf("Status = %d, want 404", apiErr.Status)
	}
	if apiErr.Code != "prompt_not_found" {
		t.Errorf("Code = %q, want prompt_not_found", apiErr.Code)
	}
}

func TestNew_RequiresBaseURLAndAPIKey(t *testing.T) {
	_, err := New(Config{APIKey: "k"})
	if err == nil {
		t.Error("expected error for missing BaseURL")
	}
	var cfgErr *ConfigError
	if !errors.As(err, &cfgErr) {
		t.Errorf("missing BaseURL error = %T, want *ConfigError", err)
	}
	if _, err := New(Config{BaseURL: "https://x"}); err == nil {
		t.Error("expected error for missing APIKey")
	} else if !errors.As(err, &cfgErr) {
		t.Errorf("missing APIKey error = %T, want *ConfigError", err)
	}
}
