import { describe, it, expect, vi } from "vitest";

import {
  AnseoObserver,
  AnseoApiError,
  AnseoConfigError,
  OpenGeoApiError,
  observe,
  observeRun,
  startRun,
  detectProviderModel,
  extractText,
  type AnseoLogger,
  type ObserveRunResult,
} from "./index.js";

const OK_BODY: ObserveRunResult = {
  run_id: "run_123",
  project_id: "proj_abc",
  prompt_slug: "best-polarized-sunglasses",
  provider: "openai",
  observed_at: "2026-06-04T12:00:00Z",
  contribution: { status: "sealed" },
};

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("AnseoObserver", () => {
  it("posts to /v1/ingest/run with auth + project headers and snake_case body", async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(OK_BODY));
    const observer = new AnseoObserver({
      baseUrl: "https://anseo.internal/",
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
    expect(url).toBe("https://anseo.internal/v1/ingest/run");
    expect(init.method).toBe("POST");

    const headers = init.headers as Record<string, string>;
    expect(headers["x-anseo-api-key"]).toBe("key-xyz");
    expect(headers["x-anseo-project"]).toBe("Sunski");
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
    const observer = new AnseoObserver({
      baseUrl: "https://anseo.internal",
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
    expect(headers["x-anseo-project"]).toBeUndefined();
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
    const observer = new AnseoObserver({
      baseUrl: "https://anseo.internal",
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
    const observer = new AnseoObserver({
      baseUrl: "https://anseo.internal",
      apiKey: "k",
      fetch: fetchMock,
    });

    const promise = observer.observeRun({
      promptSlug: "p",
      provider: "openai",
      model: "m",
    });
    // OpenGeoApiError is a back-compat alias of AnseoApiError.
    await expect(promise).rejects.toBeInstanceOf(OpenGeoApiError);
    await expect(promise).rejects.toBeInstanceOf(AnseoApiError);
    await expect(promise).rejects.toMatchObject({
      name: "AnseoApiError",
      status: 404,
      code: "prompt_not_found",
    });
  });

  it("requires baseUrl and apiKey, throwing AnseoConfigError at construction", () => {
    expect(() => new AnseoObserver({ baseUrl: "", apiKey: "k" })).toThrow(
      /baseUrl/,
    );
    expect(() => new AnseoObserver({ baseUrl: "", apiKey: "k" })).toThrow(
      AnseoConfigError,
    );
    expect(
      () => new AnseoObserver({ baseUrl: "https://x", apiKey: "" }),
    ).toThrow(/apiKey/);
  });

  it("exposes a one-shot observeRun helper", async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(OK_BODY));
    const result = await observeRun(
      { baseUrl: "https://anseo.internal", apiKey: "k", fetch: fetchMock },
      { promptSlug: "p", provider: "openai", model: "m" },
    );
    expect(result.run_id).toBe("run_123");
    expect(fetchMock).toHaveBeenCalledOnce();
  });
});

function silentLogger(): AnseoLogger {
  return { debug: vi.fn(), warn: vi.fn() };
}

describe("AnseoObserver.send (best-effort, at-most-once)", () => {
  it("returns the result on the happy path", async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(OK_BODY));
    const observer = new AnseoObserver({
      baseUrl: "https://anseo.internal",
      apiKey: "k",
      fetch: fetchMock,
      logger: silentLogger(),
    });
    const result = await observer.send({
      promptSlug: "p",
      provider: "openai",
      model: "m",
    });
    expect(result).toEqual(OK_BODY);
  });

  it("swallows a transport failure, returns null, and never retries", async () => {
    const fetchMock = vi.fn().mockRejectedValue(new Error("ECONNREFUSED"));
    const logger = silentLogger();
    const observer = new AnseoObserver({
      baseUrl: "https://anseo.internal",
      apiKey: "k",
      fetch: fetchMock,
      logger,
    });
    const result = await observer.send({
      promptSlug: "p",
      provider: "openai",
      model: "m",
    });
    expect(result).toBeNull();
    expect(fetchMock).toHaveBeenCalledTimes(1); // at-most-once: no retry
    expect(logger.debug).toHaveBeenCalled();
  });

  it("swallows a 5xx without retrying (at-most-once on server error)", async () => {
    const fetchMock = vi.fn(() =>
      Promise.resolve(jsonResponse({ error: "internal" }, 500)),
    );
    const observer = new AnseoObserver({
      baseUrl: "https://anseo.internal",
      apiKey: "k",
      fetch: fetchMock,
      logger: silentLogger(),
    });
    const result = await observer.send({
      promptSlug: "p",
      provider: "openai",
      model: "m",
    });
    expect(result).toBeNull();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("logs a 401 at WARN but still swallows it", async () => {
    const fetchMock = vi.fn(() =>
      Promise.resolve(jsonResponse({ error: "unauthorized" }, 401)),
    );
    const logger = silentLogger();
    const observer = new AnseoObserver({
      baseUrl: "https://anseo.internal",
      apiKey: "bad",
      fetch: fetchMock,
      logger,
    });
    const result = await observer.send({
      promptSlug: "p",
      provider: "openai",
      model: "m",
    });
    expect(result).toBeNull();
    expect(logger.warn).toHaveBeenCalled();
  });
});

describe("provider/model auto-detection", () => {
  it("detects OpenAI chat completions", () => {
    const resp = {
      object: "chat.completion",
      model: "gpt-4o-2024-08-06",
      choices: [{ message: { content: "Try Sunski." } }],
    };
    expect(detectProviderModel(resp)).toEqual({
      provider: "openai",
      model: "gpt-4o-2024-08-06",
    });
    expect(extractText(resp)).toBe("Try Sunski.");
  });

  it("detects the OpenAI Responses API via output_text", () => {
    const resp = { object: "response", model: "gpt-4o", output_text: "hi" };
    expect(detectProviderModel(resp).provider).toBe("openai");
    expect(extractText(resp)).toBe("hi");
  });

  it("detects Anthropic messages and concatenates text blocks", () => {
    const resp = {
      type: "message",
      model: "claude-3-5-sonnet-20241022",
      content: [
        { type: "text", text: "Hello " },
        { type: "text", text: "world" },
      ],
    };
    expect(detectProviderModel(resp)).toEqual({
      provider: "anthropic",
      model: "claude-3-5-sonnet-20241022",
    });
    expect(extractText(resp)).toBe("Hello world");
  });

  it("returns undefined provider/model for an unknown shape", () => {
    expect(detectProviderModel({ foo: "bar" })).toEqual({
      provider: undefined,
      model: undefined,
    });
  });

  it("treats a plain string as the response text", () => {
    expect(extractText("just text")).toBe("just text");
  });
});

describe("observe wrapper", () => {
  it("captures provider/model/text from the wrapped call and ships", async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(OK_BODY));
    const observer = new AnseoObserver({
      baseUrl: "https://anseo.internal",
      apiKey: "k",
      fetch: fetchMock,
      logger: silentLogger(),
    });

    const resp = {
      object: "chat.completion",
      model: "gpt-4o-2024-08-06",
      choices: [{ message: { content: "Try Sunski." } }],
    };
    const returned = await observe(
      observer,
      { promptSlug: "best-sunglasses" },
      () => resp,
    );

    expect(returned).toBe(resp); // pass-through
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    const body = JSON.parse(init.body as string) as Record<string, unknown>;
    expect(body.provider).toBe("openai");
    expect(body.model).toBe("gpt-4o-2024-08-06");
    expect(body.response_text).toBe("Try Sunski.");
    expect(body.prompt_slug).toBe("best-sunglasses");
  });

  it("sends nothing when the wrapped call throws, and propagates the error", async () => {
    const fetchMock = vi.fn();
    const observer = new AnseoObserver({
      baseUrl: "https://anseo.internal",
      apiKey: "k",
      fetch: fetchMock,
      logger: silentLogger(),
    });
    const boom = new Error("provider down");
    await expect(
      observe(observer, { promptSlug: "p" }, () => {
        throw boom;
      }),
    ).rejects.toBe(boom);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("skips the send when model cannot be determined", async () => {
    const fetchMock = vi.fn();
    const logger = silentLogger();
    const observer = new AnseoObserver({
      baseUrl: "https://anseo.internal",
      apiKey: "k",
      fetch: fetchMock,
      logger,
    });
    // Unknown shape => no model detected, none supplied.
    await observe(observer, { promptSlug: "p" }, () => ({ foo: "bar" }));
    expect(fetchMock).not.toHaveBeenCalled();
    expect(logger.debug).toHaveBeenCalled();
  });

  it("startRun supports manual capture + ship with explicit overrides", async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(OK_BODY));
    const observer = new AnseoObserver({
      baseUrl: "https://anseo.internal",
      apiKey: "k",
      fetch: fetchMock,
      logger: silentLogger(),
    });
    const run = startRun(observer, { promptSlug: "p" });
    run.capture("plain text answer", { provider: "openai", model: "gpt-4o" });
    const result = await run.ship();
    expect(result).toEqual(OK_BODY);
    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    const body = JSON.parse(init.body as string) as Record<string, unknown>;
    expect(body.response_text).toBe("plain text answer");
    expect(body.provider).toBe("openai");
    expect(body.model).toBe("gpt-4o");
  });

  it("ships nothing when startRun is never captured", async () => {
    const fetchMock = vi.fn();
    const logger = silentLogger();
    const observer = new AnseoObserver({
      baseUrl: "https://anseo.internal",
      apiKey: "k",
      fetch: fetchMock,
      logger,
    });
    const run = startRun(observer, { promptSlug: "p" });
    const result = await run.ship();
    expect(result).toBeNull();
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
