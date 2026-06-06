// wrapper.go — the Observe instrumentation surface and read-only auto-detection
// (Story 40.3).
//
// Observe wraps an existing LLM call so its run ships to Anseo without changing
// your inference logic, the Go port of the Python reference's observe
// context-manager/decorator. Delivery is best-effort and at-most-once (see
// Observer.Send): observability never returns an error into, or retries inside,
// the host app. If the wrapped call itself returns an error, nothing is sent
// (there is no run to record) and that error is returned unchanged.
//
// Per core-spec invariant #2 (no monkeypatching), auto-detection never imports
// or patches any provider SDK — it reads documented fields off whatever value
// the caller hands it. Because Go is statically typed and the provider SDK
// types are not importable here, detection operates on the idiomatic raw shapes
// a Go caller actually has: a plain string, or a JSON-decoded map[string]any.

package observe

import (
	"context"
	"strings"
)

// ObserveOptions configures an Observe call.
type ObserveOptions struct {
	// PromptSlug is the declared prompt slug for this run. Required.
	PromptSlug string
	// Provider, if set, overrides auto-detection.
	Provider string
	// Model, if set, overrides auto-detection.
	Model string
	// ObservedRank is a pre-computed brand rank for the run. Use nil to omit.
	ObservedRank *int
	// CitationDomains are pre-extracted citation domains.
	CitationDomains []string
}

// Observe runs fn, treats its returned value as the raw provider response,
// auto-detects provider/model + extracts text, and ships the run best-effort.
// It returns fn's value and error unchanged.
//
// If fn returns a non-nil error, nothing is sent. If the model cannot be
// determined (and is not supplied via opts), the run is skipped (logged at
// DEBUG) rather than sent with a bogus model.
func Observe(ctx context.Context, o *Observer, opts ObserveOptions, fn func() (any, error)) (any, error) {
	raw, err := fn()
	if err != nil {
		return raw, err // wrapped call failed => nothing to record
	}

	provider := opts.Provider
	model := opts.Model
	if provider == "" || model == "" {
		detProvider, detModel := DetectProviderModel(raw)
		if provider == "" {
			provider = detProvider
		}
		if model == "" {
			model = detModel
		}
	}

	if model == "" {
		debugf(o.logger, "could not determine model for %s; skipping send (set opts.Model explicitly)", opts.PromptSlug)
		return raw, nil
	}
	if provider == "" {
		// Server-validated sentinel when undetectable.
		provider = "unknown"
	}

	o.Send(ctx, RunInput{
		PromptSlug:      opts.PromptSlug,
		Provider:        provider,
		Model:           model,
		ResponseText:    ExtractText(raw),
		CitationDomains: opts.CitationDomains,
		ObservedRank:    opts.ObservedRank,
	})
	return raw, nil
}

// DetectProviderModel does a best-effort read of (provider, model) from a raw
// response. It returns empty strings for either field it cannot determine; the
// caller must then supply it.
//
// Supported shapes (no provider-SDK import):
//   - a JSON-decoded map[string]any (the idiomatic Go raw response): reads the
//     "object"/"type"/"model" keys per the same rules as the Python/TS SDKs.
//   - anything else: ("", "").
func DetectProviderModel(raw any) (provider string, model string) {
	m, ok := raw.(map[string]any)
	if !ok {
		return "", ""
	}

	if s, ok := m["model"].(string); ok {
		model = s
	}

	if obj, ok := m["object"].(string); ok {
		if strings.HasPrefix(obj, "chat.") || obj == "response" || obj == "text_completion" {
			return "openai", model
		}
	}
	if t, ok := m["type"].(string); ok && t == "message" {
		return "anthropic", model
	}
	if strings.HasPrefix(model, "claude") {
		return "anthropic", model
	}
	if strings.HasPrefix(model, "gpt") {
		return "openai", model
	}
	return "", model
}

// ExtractText pulls assistant text from a known OpenAI/Anthropic response shape,
// or returns a plain string as-is. Returns "" when no text can be extracted.
//
// Supported shapes (mirroring the Python/TS SDKs over the JSON-decoded form):
//   - string: returned as-is.
//   - map with "output_text" (OpenAI Responses API).
//   - map with "choices"[0]."message"."content" (OpenAI chat completions).
//   - map with "content" as a list of blocks; the ".text" of each is joined
//     (Anthropic Messages).
func ExtractText(raw any) string {
	if s, ok := raw.(string); ok {
		return s
	}
	m, ok := raw.(map[string]any)
	if !ok {
		return ""
	}

	// OpenAI Responses API convenience accessor.
	if s, ok := m["output_text"].(string); ok && s != "" {
		return s
	}

	// OpenAI chat.completions: choices[0].message.content.
	if choices, ok := m["choices"].([]any); ok && len(choices) > 0 {
		if first, ok := choices[0].(map[string]any); ok {
			if msg, ok := first["message"].(map[string]any); ok {
				if content, ok := msg["content"].(string); ok {
					return content
				}
			}
		}
	}

	// Anthropic Messages: content is a list of typed blocks; join the text ones.
	if content, ok := m["content"].([]any); ok {
		var parts []string
		for _, block := range content {
			if b, ok := block.(map[string]any); ok {
				if text, ok := b["text"].(string); ok {
					parts = append(parts, text)
				}
			}
		}
		if len(parts) > 0 {
			return strings.Join(parts, "")
		}
	}

	return ""
}
