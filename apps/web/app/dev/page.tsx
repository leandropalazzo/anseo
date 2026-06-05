import { notFound } from "next/navigation";

import { Card } from "@/components/ui/card";
import { DevBanner } from "@/components/dev/dev-banner";
import {
  DEV_PLUGIN_MOCK,
  isDevModeEnabled,
  isHostedCloud,
} from "@/lib/dev-mode";

import { PageHeader } from "@/components/ui/page-header";
import { DevOverview } from "./_components/dev-overview";

export const dynamic = "force-dynamic";

export default function DevPage() {
  // UX-DR124 — /dev refuses to render on Hosted Cloud (Phase 4 stub). The
  // refusal is explicit, not a 404, so the reason is legible.
  if (isHostedCloud()) {
    return (
      <section data-testid="dev-hosted-refusal" className="flex flex-col gap-[8px]">
        <PageHeader title="Dev mode unavailable" />
        <Card>
          <p className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
            Plugin author surfaces are disabled on Hosted Cloud. Run Anseo
            locally to load and hot-reload development plugins.
          </p>
        </Card>
      </section>
    );
  }

  // UX-DR120 — the route only exists when dev mode is enabled.
  if (!isDevModeEnabled()) notFound();

  return (
    <section data-testid="dev-page" className="flex flex-col gap-[12px]">
      <DevBanner />
      <PageHeader
        title="Plugin Dev"
        description="Hot-reload, logs, and capability inspection for locally loaded plugins."
      />
      <DevOverview state={DEV_PLUGIN_MOCK} />
    </section>
  );
}
