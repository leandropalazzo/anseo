import { describe, it, expect, vi } from "vitest";

import {
  OpenGeoObserver,
  OpenGeoApiError,
  observeRun,
  type ObserveRunResult,
} from "./index";

const OK_BODY: ObserveRunResult = {
  runId: "run_123",
  projectId: "proj_abc",
  promptSlug: "best-polarized-sunglasses",
  provider: "openai",
  observedAt: "2026-06-04T12:00:00Z",
  contribution: { status: "sealed" },
};

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("OpenGeoObserver", () => {
  it("posts to /v1/ingest/run with auth + project headers and snake_case body", async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(OK_BODY));
    const observer = new OpenGeoObserver({
      baseUrl: "https://opengeo.internal/",
      apiKey: "key-xyz",
      project: "Sunski",
      fetch: fetchMock,
    });

    const result = await observer.observeRun({
      promptSlug: "best-polarized-sunglasses",
      provider: "openai",
      model: "gpt-4o-2024-08-06",
      responseText: "Try Sunski, see https://sunski.com",
      observedRank: 1,
      observedAt: new Date("2026-06-04T12:00:00Z"),
    });

    expect(result).toEqual(OK_BODY);
    expect(fetchMock).toHaveBeenCalledTimes(1);

    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    // Trailing slash on baseUrl must be normalized, not doubled.
    expect(url).toBe("https://opengeo.internal/v1/ingest/run");
    expect(init.method).toBe("POST");

    const headers = init.headers as Record<string, string>;
    expect(headers["x-opengeo-api-key"]).toBe("key-xyz");
    expect(headers["x-opengeo-project"]).toBe("Sunski");
    expect(headers["content-type"]).toBe("application/json");

    expect(JSON.parse(init.body as string)).toEqual({
      prompt_slug: "best-polarized-sunglasses",
      provider: "openai",
      model: "gpt-4o-2024-08-06",
      response_text: "Try Sunski, see https://sunski.com",
      observed_rank: 1,
      observed_at: "2026-06-04T12:00:00.000Z",
    });
  });

  it("omits the project header and optional fields when not provided", async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(OK_BODY));
    const observer = new OpenGeoObserver({
      baseUrl: "https://opengeo.internal",
      apiKey: "key-xyz",
      fetch: fetchMock,
    });

    await observer.observeRun({
      promptSlug: "best-polarized-sunglasses",
      provider: "openai",
      model: "gpt-4o-2024-08-06",
    });

    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    const headers = init.headers as Record<string, string>;
    expect(headers["x-opengeo-project"]).toBeUndefined();
    expect(JSON.parse(init.body as string)).toEqual({
      prompt_slug: "best-polarized-sunglasses",
      provider: "openai",
      model: "gpt-4o-2024-08-06",
    });
  });

  it("surfaces the kek_missing contribution status from the response", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      jsonResponse({ ...OK_BODY, contribution: { status: "kek_missing" } }),
    );
    const observer = new OpenGeoObserver({
      baseUrl: "https://opengeo.internal",
      apiKey: "k",
      fetch: fetchMock,
    });

    const result = await observer.observeRun({
      promptSlug: "p",
      provider: "openai",
      model: "m",
    });
    expect(result.contribution).toEqual({ status: "kek_missing" });
  });

  it("throws OpenGeoApiError carrying status + code on a non-2xx response", async () => {
    // Fresh Response per call — a Response body can only be consumed once.
    const fetchMock = vi.fn(() =>
      Promise.resolve(
        jsonResponse(
          { error: "prompt_not_found", message: "prompt `p` is not declared" },
          404,
        ),
      ),
    );
    const observer = new OpenGeoObserver({
      baseUrl: "https://opengeo.internal",
      apiKey: "k",
      fetch: fetchMock,
    });

    const promise = observer.observeRun({
      promptSlug: "p",
      provider: "openai",
      model: "m",
    });
    await expect(promise).rejects.toBeInstanceOf(OpenGeoApiError);
    await expect(promise).rejects.toMatchObject({
      name: "OpenGeoApiError",
      status: 404,
      code: "prompt_not_found",
    });
  });

  it("requires baseUrl and apiKey", () => {
    expect(
      () => new OpenGeoObserver({ baseUrl: "", apiKey: "k" }),
    ).toThrow(/baseUrl/);
    expect(
      () => new OpenGeoObserver({ baseUrl: "https://x", apiKey: "" }),
    ).toThrow(/apiKey/);
  });

  it("exposes a one-shot observeRun helper", async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(OK_BODY));
    const result = await observeRun(
      { baseUrl: "https://opengeo.internal", apiKey: "k", fetch: fetchMock },
      { promptSlug: "p", provider: "openai", model: "m" },
    );
    expect(result.runId).toBe("run_123");
    expect(fetchMock).toHaveBeenCalledOnce();
  });
});
