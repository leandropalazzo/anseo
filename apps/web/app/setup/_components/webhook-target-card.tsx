"use client";

import { useState, useTransition } from "react";
import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import { Button } from "@/components/ui/button";
import { postWebhookTest } from "@/lib/api";
import type { SetupStatus, WebhookTestResult } from "@/lib/api";

interface Props {
  webhook_target: SetupStatus["webhook_target"];
}

export function WebhookTargetCard({ webhook_target }: Props) {
  const [url, setUrl] = useState("");
  const [result, setResult] = useState<WebhookTestResult | null>(null);
  const [isPending, startTransition] = useTransition();

  function handleSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const trimmed = url.trim();
    if (!trimmed) return;

    setResult(null);
    startTransition(async () => {
      const r = await postWebhookTest(trimmed);
      setResult(r);
    });
  }

  const currentUrl =
    webhook_target.configured && webhook_target.last_status !== null
      ? "(configured)"
      : null;

  return (
    <div data-testid="webhook-target-card">
    <Card
      eyebrow="integrations"
      title="Webhook Target"
      action={
        <Pill tone={webhook_target.configured ? "ok" : "neutral"}>
          <span
            aria-hidden
            className="mr-[4px] inline-block h-[6px] w-[6px] rounded-full"
            style={{
              background: webhook_target.configured
                ? "var(--ok)"
                : "var(--text-faint)",
            }}
          />
          {webhook_target.configured ? "configured" : "not set"}
        </Pill>
      }
    >
      <div className="flex flex-col gap-[12px]">
        {/* Current status rows */}
        <div className="flex flex-col gap-[6px]">
          {currentUrl && (
            <div className="flex items-center justify-between gap-[8px]">
              <span className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
                Target URL
              </span>
              <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)]">
                {currentUrl}
              </span>
            </div>
          )}
          {webhook_target.last_delivery_at && (
            <div className="flex items-center justify-between gap-[8px]">
              <span className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
                Last Delivery
              </span>
              <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)]">
                {new Date(webhook_target.last_delivery_at).toLocaleString()}
              </span>
            </div>
          )}
          {webhook_target.last_status && (
            <div className="flex items-center justify-between gap-[8px]">
              <span className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
                Last Status
              </span>
              <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)]">
                {webhook_target.last_status}
              </span>
            </div>
          )}
          {webhook_target.error && (
            <p className="m-0 text-[length:var(--font-size-xs)] text-[color:var(--danger)]">
              {webhook_target.error}
            </p>
          )}
        </div>

        {/* Test form */}
        <form
          onSubmit={handleSubmit}
          className="flex flex-col gap-[8px]"
          aria-label="Test webhook target"
        >
          <label
            htmlFor="webhook-url"
            className="text-[length:var(--font-size-xs)] text-[color:var(--text-muted)] font-medium"
          >
            Test URL
          </label>
          <div className="flex gap-[8px]">
            <input
              id="webhook-url"
              data-testid="webhook-url-input"
              type="url"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://example.com/webhook"
              required
              disabled={isPending}
              className="min-w-0 flex-1 border border-[color:var(--border)] bg-[color:var(--bg-elev-2)] px-[8px] py-[5px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)] placeholder:text-[color:var(--text-faint)] disabled:opacity-50 outline-none focus:border-[color:var(--accent)]"
            />
            <Button
              data-testid="webhook-test-button"
              type="submit"
              variant="secondary"
              size="sm"
              disabled={isPending || url.trim() === ""}
            >
              {isPending ? "Testing…" : "Test"}
            </Button>
          </div>
        </form>

        {/* Result panel */}
        {result !== null && (
          <div
            data-testid="webhook-test-result"
            className="flex flex-col gap-[6px] border border-[color:var(--hairline)] p-[10px]"
            role="region"
            aria-label="Webhook test result"
            aria-live="polite"
          >
            {result.error ? (
              <p className="m-0 text-[length:var(--font-size-xs)] text-[color:var(--danger)]">
                Error: {result.error}
              </p>
            ) : (
              <>
                <div className="flex items-center justify-between gap-[8px]">
                  <span className="text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                    Response Code
                  </span>
                  <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)]">
                    {result.status_code ?? "—"}
                  </span>
                </div>

                {result.latency_ms !== null && (
                  <div className="flex items-center justify-between gap-[8px]">
                    <span className="text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                      Latency
                    </span>
                    <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text)]">
                      {result.latency_ms}ms
                    </span>
                  </div>
                )}

                <div className="flex items-center justify-between gap-[8px]">
                  <span className="text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                    Signature Valid
                  </span>
                  <span
                    data-testid="webhook-signature-status"
                    className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)]"
                    style={{
                      color:
                        result.signature_valid === true
                          ? "var(--ok)"
                          : result.signature_valid === false
                            ? "var(--danger)"
                            : "var(--text-faint)",
                    }}
                  >
                    {result.signature_valid === null
                      ? "—"
                      : String(result.signature_valid)}
                  </span>
                </div>
              </>
            )}
          </div>
        )}
      </div>
    </Card>
    </div>
  );
}
