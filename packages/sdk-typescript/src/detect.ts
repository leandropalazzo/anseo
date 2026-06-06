/**
 * Read-only provider/model auto-detection and response-text extraction.
 *
 * Mirrors `_detect_provider_model` / `_extract_text` in the Python reference
 * (`anseo_observe`). Per core-spec invariant #2 (no monkeypatching), this never
 * imports, patches, or wraps the OpenAI/Anthropic SDKs — it only reads
 * documented attributes off whatever response object the caller hands it.
 *
 * Attribute paths (and their SDK version floor):
 *
 * - **OpenAI** (`openai>=1.0`): `response.object` is `"chat.completion"` /
 *   `"response"` / `"text_completion"`; `response.model` is e.g.
 *   `"gpt-4o-2024-08-06"`. Text comes from `response.output_text` (Responses
 *   API) or `response.choices[0].message.content` (chat completions).
 * - **Anthropic** (`anthropic>=0.21`): `response.type === "message"` or the
 *   model starts with `claude`; text is the concatenated `.text` of each block
 *   in `response.content`.
 * - A plain `string` is treated as the response text itself.
 */

/** A duck-typed view of the attributes we read off a raw response object. */
interface RawResponse {
  model?: unknown;
  object?: unknown;
  type?: unknown;
  output_text?: unknown;
  choices?: unknown;
  content?: unknown;
}

function asString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

/**
 * Best-effort `(provider, model)` from a raw response. Either field is
 * `undefined` when it cannot be determined; the caller must then supply it.
 */
export function detectProviderModel(raw: unknown): {
  provider: string | undefined;
  model: string | undefined;
} {
  if (raw === null || typeof raw !== "object") {
    return { provider: undefined, model: undefined };
  }
  const r = raw as RawResponse;
  const model = asString(r.model);

  let provider: string | undefined;
  const obj = asString(r.object);
  const rtype = asString(r.type);
  if (obj && (obj.startsWith("chat.") || obj === "response" || obj === "text_completion")) {
    provider = "openai";
  } else if (rtype === "message" || (model && model.startsWith("claude"))) {
    provider = "anthropic";
  } else if (model && model.startsWith("gpt")) {
    provider = "openai";
  }

  return { provider, model };
}

/**
 * Pull assistant text from a known OpenAI/Anthropic response shape, or return
 * a plain string as-is. Returns `undefined` when no text can be extracted.
 */
export function extractText(raw: unknown): string | undefined {
  if (raw === null || raw === undefined) return undefined;
  if (typeof raw === "string") return raw;
  if (typeof raw !== "object") return undefined;
  const r = raw as RawResponse;

  // OpenAI Responses API convenience accessor.
  const outputText = asString(r.output_text);
  if (outputText) return outputText;

  // OpenAI chat.completions: choices[0].message.content.
  if (Array.isArray(r.choices) && r.choices.length > 0) {
    const first = r.choices[0] as { message?: { content?: unknown } } | undefined;
    const content = first?.message?.content;
    if (typeof content === "string") return content;
  }

  // Anthropic Messages: content is a list of typed blocks; join the text ones.
  if (Array.isArray(r.content)) {
    const parts: string[] = [];
    for (const block of r.content) {
      const text = (block as { text?: unknown } | null)?.text;
      if (typeof text === "string") parts.push(text);
    }
    if (parts.length > 0) return parts.join("");
  }

  return undefined;
}
