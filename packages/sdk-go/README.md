# observe (Go)

Thin instrumentation SDK to send **externally-executed** LLM runs to Anseo's
Run-Ingestion API (`POST /v1/ingest/run`). The OpenTelemetry pattern, minus the
ceremony: you already ran a prompt against a provider outside Anseo — this
posts that run so it flows through the same extraction → redaction →
benchmark-contribution path as a native run.

Go port of the Python reference (`packages/sdk-python/anseo_observe`),
implementing the same language-agnostic spec in `docs/sdk-spec.md`.

- **Standard library only.** Go 1.22+.
- Sends the API key as `X-Anseo-API-Key` and scopes the run with
  `X-Anseo-Project` (brand name).
- **Best-effort, at-most-once**: `Send`/`Observe` never return an error into
  your app and never retry.

## Install

```bash
go get github.com/opengeo/opengeo/packages/sdk-go
```

## Strict client (read each contribution status)

```go
import observe "github.com/opengeo/opengeo/packages/sdk-go"

observer, _ := observe.New(observe.Config{
    BaseURL: "https://anseo.internal",
    APIKey:  os.Getenv("ANSEO_API_KEY"),
    Project: "Sunski", // omit for single-project deployments
})

result, err := observer.ObserveRun(ctx, observe.RunInput{
    PromptSlug:   "best-polarized-sunglasses",
    Provider:     "openai",
    Model:        "gpt-4o-2024-08-06",
    ResponseText: completionText,
    // optional:
    // CitationDomains: []string{"sunski.com"},
    // ObservedRank:    observe.Int(1),
    // ObservedAt:      time.Now(),
})
if err != nil {
    // *observe.APIError carries .Status and .Code on non-2xx responses.
    // New returns *observe.ConfigError for a missing BaseURL/APIKey.
    log.Fatal(err)
}

// result.Contribution tells you whether benchmark data was sealed:
// "sealed" | "skipped_not_opted_in" | "kek_missing" | "redaction_rejected" (+ Reason)
fmt.Println(result.RunID, result.Contribution.Status)
```

## Wrap a call (auto-detect, best-effort)

`Observe` runs your call, auto-detects provider/model + extracts text from a
JSON-decoded response (`map[string]any`) or a plain string, and ships the run
best-effort. It returns your value/error unchanged; if the call errors, nothing
is sent.

```go
returned, err := observe.Observe(ctx, observer,
    observe.ObserveOptions{PromptSlug: "best-polarized-sunglasses"},
    func() (any, error) {
        return callMyLLM(ctx) // returns the decoded response + error
    },
)
```

`observer.Send(ctx, input)` is the lower-level best-effort surface (returns the
result or nil; never errors). Enable diagnostics with `DEBUG=anseo`.

## Develop / test

```bash
go vet ./...
staticcheck ./...
go test ./...
```
