import {
  fetchAlertRules,
  fetchAnomalies,
  type AlertRule,
  type AnomalyItem,
} from "@/lib/api";

import { PageHeader } from "@/components/ui/page-header";
import { AlertsView } from "./_components/alerts-view";

/**
 * Alerts route — inbox of anomalies and the rules that fired them. Schedules
 * now live in their own Operate section at `/schedules`.
 */
export default async function AlertsPage() {
  // Live anomaly inbox (last 7d) + live alert rules. Each is best-effort:
  // an unreachable API leaves the list empty and the component renders its
  // own EmptyState rather than failing the whole page.
  let incidents: ReadonlyArray<AnomalyItem> = [];
  try {
    incidents = await fetchAnomalies("7d");
  } catch {
    incidents = [];
  }

  let rules: ReadonlyArray<AlertRule> = [];
  try {
    rules = await fetchAlertRules();
  } catch {
    rules = [];
  }

  return (
    <section data-testid="alerts-page" className="flex flex-col gap-[12px]">
      <PageHeader
        title="Alerts"
        description="Inbox of anomalies and the rules that fired them."
      />
      <AlertsView incidents={incidents} rules={rules} />
    </section>
  );
}
