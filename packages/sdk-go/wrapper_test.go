package observe

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"
)

// failingTransport always errors, simulating a network failure.
type failingTransport struct{ calls int32 }

func (f *failingTransport) RoundTrip(*http.Request) (*http.Response, error) {
	atomic.AddInt32(&f.calls, 1)
	return nil, errors.New("dial tcp: connection refused")
}

func TestSend_HappyPathReturnsResult(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = io.WriteString(w, okBody)
	}))
	defer srv.Close()

	observer, _ := New(Config{BaseURL: srv.URL, APIKey: "k"})
	res := observer.Send(context.Background(), RunInput{PromptSlug: "p", Provider: "openai", Model: "m"})
	if res == nil {
		t.Fatal("Send returned nil on the happy path")
	}
	if res.RunID != "run_123" {
		t.Errorf("RunID = %q", res.RunID)
	}
}

func TestSend_SwallowsTransportFailureAndDoesNotRetry(t *testing.T) {
	ft := &failingTransport{}
	observer, _ := New(Config{
		BaseURL:    "https://anseo.internal",
		APIKey:     "k",
		HTTPClient: &http.Client{Transport: ft},
	})
	res := observer.Send(context.Background(), RunInput{PromptSlug: "p", Provider: "openai", Model: "m"})
	if res != nil {
		t.Errorf("Send should return nil on transport failure, got %v", res)
	}
	if got := atomic.LoadInt32(&ft.calls); got != 1 {
		t.Errorf("transport called %d times, want exactly 1 (at-most-once: no retry)", got)
	}
}

func TestSend_SwallowsServerErrorWithoutRetry(t *testing.T) {
	var calls int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		atomic.AddInt32(&calls, 1)
		w.WriteHeader(http.StatusInternalServerError)
		_, _ = io.WriteString(w, `{"error":"internal"}`)
	}))
	defer srv.Close()

	observer, _ := New(Config{BaseURL: srv.URL, APIKey: "k"})
	res := observer.Send(context.Background(), RunInput{PromptSlug: "p", Provider: "openai", Model: "m"})
	if res != nil {
		t.Errorf("Send should return nil on 5xx, got %v", res)
	}
	if got := atomic.LoadInt32(&calls); got != 1 {
		t.Errorf("server hit %d times, want 1 (at-most-once on 5xx)", got)
	}
}

func TestSend_Swallows401(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusUnauthorized)
		_, _ = io.WriteString(w, `{"error":"unauthorized","message":"bad key"}`)
	}))
	defer srv.Close()

	observer, _ := New(Config{BaseURL: srv.URL, APIKey: "bad"})
	if res := observer.Send(context.Background(), RunInput{PromptSlug: "p", Provider: "openai", Model: "m"}); res != nil {
		t.Errorf("Send should swallow a 401, got %v", res)
	}
}

func TestDetectProviderModel(t *testing.T) {
	cases := []struct {
		name      string
		raw       any
		wantProv  string
		wantModel string
	}{
		{
			name:      "openai chat completion",
			raw:       map[string]any{"object": "chat.completion", "model": "gpt-4o-2024-08-06"},
			wantProv:  "openai",
			wantModel: "gpt-4o-2024-08-06",
		},
		{
			name:      "openai responses api",
			raw:       map[string]any{"object": "response", "model": "gpt-4o"},
			wantProv:  "openai",
			wantModel: "gpt-4o",
		},
		{
			name:      "anthropic message",
			raw:       map[string]any{"type": "message", "model": "claude-3-5-sonnet-20241022"},
			wantProv:  "anthropic",
			wantModel: "claude-3-5-sonnet-20241022",
		},
		{
			name:      "anthropic via model prefix only",
			raw:       map[string]any{"model": "claude-3-haiku"},
			wantProv:  "anthropic",
			wantModel: "claude-3-haiku",
		},
		{
			name:      "unknown shape",
			raw:       map[string]any{"foo": "bar"},
			wantProv:  "",
			wantModel: "",
		},
		{
			name:      "non-map passthrough",
			raw:       "just a string",
			wantProv:  "",
			wantModel: "",
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			gotProv, gotModel := DetectProviderModel(tc.raw)
			if gotProv != tc.wantProv {
				t.Errorf("provider = %q, want %q", gotProv, tc.wantProv)
			}
			if gotModel != tc.wantModel {
				t.Errorf("model = %q, want %q", gotModel, tc.wantModel)
			}
		})
	}
}

func TestExtractText(t *testing.T) {
	cases := []struct {
		name string
		raw  any
		want string
	}{
		{"plain string", "hello", "hello"},
		{"openai responses output_text", map[string]any{"output_text": "hi there"}, "hi there"},
		{
			name: "openai chat content",
			raw: map[string]any{
				"choices": []any{
					map[string]any{"message": map[string]any{"content": "Try Sunski."}},
				},
			},
			want: "Try Sunski.",
		},
		{
			name: "anthropic blocks joined",
			raw: map[string]any{
				"content": []any{
					map[string]any{"type": "text", "text": "Hello "},
					map[string]any{"type": "text", "text": "world"},
				},
			},
			want: "Hello world",
		},
		{"unknown shape", map[string]any{"foo": "bar"}, ""},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := ExtractText(tc.raw); got != tc.want {
				t.Errorf("ExtractText = %q, want %q", got, tc.want)
			}
		})
	}
}

func TestObserve_CapturesAndShips(t *testing.T) {
	var gotBody map[string]any
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		_ = json.Unmarshal(raw, &gotBody)
		_, _ = io.WriteString(w, okBody)
	}))
	defer srv.Close()

	observer, _ := New(Config{BaseURL: srv.URL, APIKey: "k"})
	resp := map[string]any{
		"object": "chat.completion",
		"model":  "gpt-4o-2024-08-06",
		"choices": []any{
			map[string]any{"message": map[string]any{"content": "Try Sunski."}},
		},
	}
	returned, err := Observe(context.Background(), observer, ObserveOptions{PromptSlug: "best-sunglasses"}, func() (any, error) {
		return resp, nil
	})
	if err != nil {
		t.Fatalf("Observe: %v", err)
	}
	if returned == nil {
		t.Fatal("Observe should pass through the wrapped value")
	}
	if gotBody["provider"] != "openai" {
		t.Errorf("provider = %v, want openai", gotBody["provider"])
	}
	if gotBody["model"] != "gpt-4o-2024-08-06" {
		t.Errorf("model = %v", gotBody["model"])
	}
	if gotBody["response_text"] != "Try Sunski." {
		t.Errorf("response_text = %v", gotBody["response_text"])
	}
	if gotBody["prompt_slug"] != "best-sunglasses" {
		t.Errorf("prompt_slug = %v", gotBody["prompt_slug"])
	}
}

func TestObserve_WrappedCallErrorSendsNothing(t *testing.T) {
	var hit int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		atomic.AddInt32(&hit, 1)
		_, _ = io.WriteString(w, okBody)
	}))
	defer srv.Close()

	observer, _ := New(Config{BaseURL: srv.URL, APIKey: "k"})
	boom := errors.New("provider down")
	_, err := Observe(context.Background(), observer, ObserveOptions{PromptSlug: "p"}, func() (any, error) {
		return nil, boom
	})
	if !errors.Is(err, boom) {
		t.Errorf("err = %v, want the wrapped call's error", err)
	}
	if atomic.LoadInt32(&hit) != 0 {
		t.Error("nothing should be sent when the wrapped call errors")
	}
}

func TestObserve_SkipsWhenModelUndetermined(t *testing.T) {
	var hit int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		atomic.AddInt32(&hit, 1)
		_, _ = io.WriteString(w, okBody)
	}))
	defer srv.Close()

	observer, _ := New(Config{BaseURL: srv.URL, APIKey: "k"})
	_, err := Observe(context.Background(), observer, ObserveOptions{PromptSlug: "p"}, func() (any, error) {
		return map[string]any{"foo": "bar"}, nil // no model detectable
	})
	if err != nil {
		t.Fatalf("Observe: %v", err)
	}
	if atomic.LoadInt32(&hit) != 0 {
		t.Error("no send should happen when the model cannot be determined")
	}
}
