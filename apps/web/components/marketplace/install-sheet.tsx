"use client";

import { useState } from "react";

import { Button } from "@/components/ui/button";
import {
  installPlugin,
  type InstallResult,
  type MarketplacePlugin,
} from "@/lib/api";
import { capabilityLabel } from "@/lib/plugin-format";

// Story 17.8 — install Sheet + unsigned-install Dialog + permissions gate.
//   UX-DR91   permissions block renders BEFORE [INSTALL →]; button disabled
//             until the all-or-nothing acknowledgment is checked.
//   OQ-P3-26  single all-or-nothing toggle — no granular permission surface.
//   UX-DR101  unsigned plugins require an explicit ⚠ confirmation Dialog.
//   UX-DR92   signing failures render a structured ErrorBanner naming the kind.
//   UX-DR95   success surfaces the recorded Audit Event id.

const ERROR_COPY: Record<string, string> = {
  signing_failed: "Signature verification failed for this plugin.",
  capability_denied: "A requested capability was denied by policy.",
  revoked: "The publisher key has been revoked — install blocked.",
  network: "Couldn't reach the registry to complete the install.",
};

export function InstallSheet({ plugin }: { plugin: MarketplacePlugin }) {
  const [open, setOpen] = useState(false);
  const [acknowledged, setAcknowledged] = useState(false);
  const [confirmingUnsigned, setConfirmingUnsigned] = useState(false);
  const [pending, setPending] = useState(false);
  const [result, setResult] = useState<InstallResult | null>(null);

  const isUnsigned = plugin.signature_status === "unsigned";
  const isRevoked = plugin.signature_status === "revoked";

  function reset() {
    setOpen(false);
    setAcknowledged(false);
    setConfirmingUnsigned(false);
    setResult(null);
  }

  async function doInstall(acknowledgeUnsigned: boolean) {
    setPending(true);
    setConfirmingUnsigned(false);
    try {
      const res = await installPlugin(plugin.slug, {
        acknowledge_unsigned: acknowledgeUnsigned,
      });
      setResult(res);
    } finally {
      setPending(false);
    }
  }

  function onInstallClick() {
    if (isUnsigned) {
      setConfirmingUnsigned(true);
      return;
    }
    void doInstall(false);
  }

  return (
    <div data-testid="install-sheet-root">
      <Button
        variant="primary"
        size="sm"
        data-testid="open-install-sheet"
        disabled={isRevoked}
        onClick={() => setOpen(true)}
      >
        INSTALL →
      </Button>

      {open && (
        <div
          role="dialog"
          aria-modal="true"
          aria-label={`Install ${plugin.name}`}
          data-testid="install-sheet"
          className="fixed inset-y-0 right-0 z-50 flex w-[420px] max-w-[90vw] flex-col gap-[12px] overflow-auto border-l border-[color:var(--border)] bg-[color:var(--bg-elev)] p-[16px]"
        >
          <div className="flex items-center justify-between">
            <h2 className="m-0 text-[length:var(--font-size-base)] font-medium text-[color:var(--text)]">
              Install {plugin.slug}@{plugin.version}
            </h2>
            <Button
              variant="ghost"
              size="sm"
              data-testid="install-sheet-close"
              onClick={reset}
            >
              ✕
            </Button>
          </div>

          {/* UX-DR91 — permissions render BEFORE the install action. */}
          <div
            data-testid="install-permissions"
            className="flex flex-col gap-[6px]"
          >
            <div className="label-eyebrow text-[color:var(--text-faint)]">
              permissions requested
            </div>
            {plugin.capabilities.length === 0 ? (
              <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                none
              </div>
            ) : (
              <ul className="m-0 flex list-none flex-col gap-[4px] p-0">
                {plugin.capabilities.map((cap) => (
                  <li
                    key={cap.kind}
                    className="border border-[color:var(--border)] bg-[color:var(--bg-elev-2)] px-[8px] py-[4px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]"
                  >
                    {capabilityLabel(cap)}
                  </li>
                ))}
              </ul>
            )}
          </div>

          {/* OQ-P3-26 — single all-or-nothing acknowledgment. */}
          <label className="flex items-center gap-[8px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
            <input
              type="checkbox"
              data-testid="install-acknowledge"
              checked={acknowledged}
              onChange={(e) => setAcknowledged(e.target.checked)}
            />
            Grant all listed permissions
          </label>

          {result && !result.ok && (
            <div
              data-testid="install-error"
              data-error-kind={result.error_kind}
              role="alert"
              className="border border-[color:var(--danger)] bg-[color:color-mix(in_oklch,var(--danger)_8%,transparent)] px-[10px] py-[6px] text-[length:var(--font-size-sm)] text-[color:var(--danger)]"
            >
              {result.error_kind
                ? ERROR_COPY[result.error_kind]
                : result.message}
            </div>
          )}

          {result?.ok ? (
            <div
              data-testid="install-success"
              className="border border-[color:var(--ok)] bg-[color:color-mix(in_oklch,var(--ok)_8%,transparent)] px-[10px] py-[6px] text-[length:var(--font-size-sm)] text-[color:var(--text)]"
            >
              Installed. Audit event{" "}
              <span
                data-testid="install-audit-event"
                className="font-[family-name:var(--font-mono)]"
              >
                {result.audit_event_id}
              </span>
              .
            </div>
          ) : (
            <Button
              variant="primary"
              size="md"
              data-testid="install-confirm"
              disabled={!acknowledged || pending}
              onClick={onInstallClick}
            >
              {pending ? "Installing…" : "INSTALL →"}
            </Button>
          )}

          {/* UX-DR101 — unsigned confirmation Dialog. */}
          {confirmingUnsigned && (
            <div
              role="alertdialog"
              aria-modal="true"
              data-testid="unsigned-dialog"
              className="fixed inset-0 z-[60] flex items-center justify-center bg-[color:color-mix(in_oklch,black_40%,transparent)]"
            >
              <div className="flex w-[360px] max-w-[90vw] flex-col gap-[10px] border border-[color:var(--warn)] bg-[color:var(--bg-elev)] p-[16px]">
                <div className="text-[length:var(--font-size-base)] font-medium text-[color:var(--warn)]">
                  ⚠ Unsigned plugin
                </div>
                <p className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
                  This plugin has no verified signature. Installing it will
                  record an Audit Event with{" "}
                  <code className="font-[family-name:var(--font-mono)]">
                    signature_status = &quot;unsigned&quot;
                  </code>
                  . Continue only if you trust the source.
                </p>
                <div className="flex justify-end gap-[6px]">
                  <Button
                    variant="ghost"
                    size="sm"
                    data-testid="unsigned-cancel"
                    onClick={() => setConfirmingUnsigned(false)}
                  >
                    Cancel
                  </Button>
                  <Button
                    variant="danger"
                    size="sm"
                    data-testid="unsigned-confirm"
                    onClick={() => void doInstall(true)}
                  >
                    Install anyway
                  </Button>
                </div>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
