# observe (Go)

Thin instrumentation SDK to send **externally-executed** LLM runs to OpenGEO's
Run-Ingestion API (`POST /v1/ingest/run`). The OpenTelemetry pattern, minus the
ceremony: you already ran a prompt against a provider outside OpenGEO — this
posts that run so it flows through the same extraction → redaction →
benchmark-contribution path as a native run.

- **Standard library only.**
- Sends the API key as `X-OpenGEO-API-Key` and scopes the run with
  `X-OpenGEO-Project` (brand name).

## Install

```bash
go get github.com/opengeo/opengeo/packages/sdk-go
```

## One-liner integration

```go
import observe "github.com/opengeo/opengeo/packages/sdk-go"

observer, _ := observe.New(observe.Config{
    BaseURL: "https://opengeo.internal",
    APIKey:  os.Getenv("OPENGEO_API_KEY"),
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
    log.Fatal(err)
}

// result.Contribution tells you whether benchmark data was sealed:
// "sealed" | "skipped_not_opted_in" | "kek_missing" | "redaction_rejected" (+ Reason)
fmt.Println(result.RunID, result.Contribution.Status)
```

## Develop / test

```bash
go vet ./...
go test ./...
```
