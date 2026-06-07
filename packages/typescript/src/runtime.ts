// Anseo TypeScript SDK — runtime mutator referenced by orval (Story 12.3).
//
// orval's fetch client invokes the mutator as `fetchClient(url, init)`,
// where `url` is a complete relative path (e.g. "/v1/runs?limit=10")
// and `init` is a standard RequestInit augmented with the method. This
// module wraps that into base-URL composition, API-key injection, and
// error translation so callers only need to call `configure(...)` once.
//
// The API authenticates with `X-Anseo-API-Key` (architecture §5.1),
// not `Authorization: Bearer`.

export type AnseoConfig = {
  baseUrl: string;
  apiKey?: string;
  fetch?: typeof globalThis.fetch;
};

let config: AnseoConfig = {
  baseUrl: "http://127.0.0.1:8080",
};

export function configure(next: Partial<AnseoConfig>): void {
  config = { ...config, ...next };
}

export function currentConfig(): Readonly<AnseoConfig> {
  return config;
}

export class AnseoApiError extends Error {
  readonly status: number;
  readonly body: unknown;

  constructor(message: string, status: number, body: unknown) {
    super(message);
    this.name = "AnseoApiError";
    this.status = status;
    this.body = body;
  }
}

// Bodies whose Content-Type must NOT be forced to application/json.
// FormData has a generated `multipart/form-data; boundary=...` that
// only the fetch implementation knows; URLSearchParams should send
// application/x-www-form-urlencoded; Blob/ArrayBuffer/ReadableStream
// carry their own intent.
function bodyHasIntrinsicContentType(body: BodyInit | null | undefined): boolean {
  if (body == null) return false;
  if (typeof FormData !== "undefined" && body instanceof FormData) return true;
  if (typeof URLSearchParams !== "undefined" && body instanceof URLSearchParams) return true;
  if (typeof Blob !== "undefined" && body instanceof Blob) return true;
  if (typeof ArrayBuffer !== "undefined" && body instanceof ArrayBuffer) return true;
  if (typeof ReadableStream !== "undefined" && body instanceof ReadableStream) return true;
  return false;
}

// Orval 8's generated client types each response as `{ data, status, headers }`
// (modelling non-2xx as typed variants too). This SDK keeps its DOCUMENTED
// contract — HTTP errors throw `AnseoApiError`, callers `try/catch` — rather
// than returning non-2xx values, so existing consumers don't silently stop
// handling 401/404/5xx. We therefore: (a) throw on non-2xx, and (b) include
// `headers` on the SUCCESS return so the generated success-variant type is
// satisfied. The generated non-2xx variants are a harmless type-superset that
// this runtime never actually returns (it throws first). Transport-level
// failures (network/abort) also throw AnseoApiError(status=0).
export async function fetchClient<T>(
  url: string,
  init: RequestInit,
): Promise<T> {
  const fetchImpl = config.fetch ?? globalThis.fetch;
  const root = config.baseUrl.replace(/\/$/, "");
  const absoluteUrl = `${root}${url.startsWith("/") ? url : `/${url}`}`;
  const headers: Record<string, string> = {
    Accept: "application/json",
  };
  if (init.headers) {
    if (init.headers instanceof Headers) {
      init.headers.forEach((value, key) => {
        headers[key] = value;
      });
    } else if (Array.isArray(init.headers)) {
      for (const [key, value] of init.headers) headers[key] = value;
    } else {
      Object.assign(headers, init.headers as Record<string, string>);
    }
  }
  if (init.body !== undefined && init.body !== null && !bodyHasIntrinsicContentType(init.body)) {
    headers["Content-Type"] ??= "application/json";
  }
  if (config.apiKey) {
    headers["X-Anseo-API-Key"] ??= config.apiKey;
  }

  let response: Response;
  try {
    response = await fetchImpl(absoluteUrl, { ...init, headers });
  } catch (cause) {
    // Network errors (DNS, TLS, abort, offline) bypass the HTTP layer
    // entirely. Wrap them in AnseoApiError(status=0) so consumers can
    // pattern-match on a single exception type for all failure modes.
    throw new AnseoApiError(
      `network error: ${cause instanceof Error ? cause.message : String(cause)}`,
      0,
      cause,
    );
  }
  const text = await response.text();
  const parsed = text.length === 0 ? undefined : safeParseJson(text);
  if (!response.ok) {
    // Documented contract: HTTP errors throw AnseoApiError (callers try/catch).
    throw new AnseoApiError(
      `Anseo API ${init.method ?? "GET"} ${url} failed: ${response.status}`,
      response.status,
      parsed,
    );
  }
  // Success: include `headers` so the generated success-variant type matches.
  return { data: parsed, status: response.status, headers: response.headers } as T;
}

function safeParseJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}
