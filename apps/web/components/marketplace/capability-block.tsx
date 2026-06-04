import type { PluginCapability } from "@/lib/api";
import { capabilityLabel } from "@/lib/plugin-format";

// UX-DR94 — capability disclosure is an always-visible block, never a
// collapsible/accordion. The operator must see exactly what a plugin can do
// before installing, with no interaction required to reveal it.
export function CapabilityBlock({
  capabilities,
}: {
  capabilities: PluginCapability[];
}) {
  return (
    <div data-testid="plugin-capabilities" className="flex flex-col gap-[6px]">
      <div className="label-eyebrow text-[color:var(--text-faint)]">
        capabilities requested
      </div>
      {capabilities.length === 0 ? (
        <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
          none — this plugin requests no host capabilities
        </div>
      ) : (
        <ul className="m-0 flex list-none flex-col gap-[4px] p-0">
          {capabilities.map((cap) => (
            <li
              key={cap.kind}
              data-testid="plugin-capability"
              data-kind={cap.kind}
              className="border border-[color:var(--border)] bg-[color:var(--bg-elev-2)] px-[8px] py-[4px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]"
            >
              {capabilityLabel(cap)}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
