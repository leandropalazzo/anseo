import {
  fetchDeclaredPrompts,
  fetchSchedules,
  fetchSetupStatus,
  type ScheduleSummary,
} from "@/lib/api";

import { Card } from "@/components/ui/card";
import { PageHeader } from "@/components/ui/page-header";
import { configuredConcreteProviderIds } from "@/lib/provider-colors";
import { ScheduleGrid } from "../alerts/_components/schedule-grid";

/**
 * Schedules — the operator surface for declaring and running prompt × provider
 * matrices on a cadence. Lives as its own Operate section (was previously the
 * third tab on /alerts).
 */
export default async function SchedulesPage() {
  let schedules: ReadonlyArray<ScheduleSummary> = [];
  let apiError: string | null = null;
  try {
    const r = await fetchSchedules();
    schedules = r.schedules;
  } catch (e) {
    apiError = e instanceof Error ? e.message : String(e);
  }
  const declaredPrompts = await fetchDeclaredPrompts();

  // Provider wire names with a stored key — the create form defaults to fanning
  // out across all of them. Best-effort: an unreachable API yields an empty
  // list and the form falls back to its own default.
  let configuredProviders: string[] = [];
  try {
    const status = await fetchSetupStatus();
    configuredProviders = configuredConcreteProviderIds(status.api_keys);
  } catch {
    configuredProviders = [];
  }

  return (
    <section data-testid="schedules-page" className="flex flex-col gap-[12px]">
      <PageHeader
        title="Schedules"
        description="Declare prompt × provider matrices on a cadence, or trigger one on demand with Run now."
      />
      <Card padding={false} title="Schedules">
        <ScheduleGrid
          schedules={schedules}
          declaredPrompts={declaredPrompts}
          configuredProviders={configuredProviders}
          apiError={apiError}
        />
      </Card>
    </section>
  );
}
