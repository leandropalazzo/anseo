import { describe, expect, it } from "vitest";

import {
  configuredConcreteProviderIds,
  configuredCredentialProviderIds,
} from "./provider-colors";

describe("configured provider helpers", () => {
  it("expands OpenRouter for concrete provider surfaces", () => {
    const keys = [{ provider: "openrouter", configured: true }];

    expect(configuredConcreteProviderIds(keys)).toEqual([
      "openai",
      "anthropic",
      "gemini",
      "perplexity",
      "grok",
      "mistral",
    ]);
  });

  it("keeps OpenRouter as the credential route for AI suggestion calls", () => {
    const keys = [
      { provider: "openai", configured: false },
      { provider: "openrouter", configured: true },
    ];

    expect(configuredCredentialProviderIds(keys)).toEqual(["openrouter"]);
  });
});
