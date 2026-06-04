import {
  fetchSetupStatus,
  fetchClickHouseEtlStatus,
  type ClickHouseEtlStatus,
} from "@/lib/api";

import { PostgresCard } from "./_components/postgres-card";
import { WorkerCard } from "./_components/worker-card";
import { ClickHouseCard } from "./_components/clickhouse-card";
import { EtlProgressCard } from "./_components/etl-progress-card";
import { ApiKeysCard } from "./_components/api-keys-card";
import { WebhookTargetCard } from "./_components/webhook-target-card";

const ETL_UNAVAILABLE: ClickHouseEtlStatus = {
  state: "unknown",
  batches_done: null,
  batches_total: null,
  last_heartbeat_at: null,
  finished_at: null,
  error: null,
};

export default async function SetupPage({
  searchParams,
}: {
  searchParams: Promise<{ empty?: string }>;
}) {
  // `?empty=1` is an E2E-only affordance: it forwards to the mock backend so
  // the Playwright empty-state spec can exercise the no-keys / unconfigured
  // path through SSR. The live API ignores it.
  const sp = await searchParams;
  const mockEmpty = sp.empty === "1";

  let status;
  let fetchError: string | null = null;

  // Best-effort: the API being down must not 500 the page. `fetchSetupStatus`
  // is guarded below; the ETL status fetch (live since story 30-8f) likewise
  // degrades to the `unknown` state the EtlProgressCard already renders.
  let etl: ClickHouseEtlStatus;
  try {
    etl = await fetchClickHouseEtlStatus();
  } catch {
    etl = ETL_UNAVAILABLE;
  }

  try {
    status = await fetchSetupStatus({ mockEmpty });
  } catch (err) {
    fetchError =
      err instanceof Error ? err.message : "Failed to load setup status.";
  }

  return (
    <div data-testid="setup-page" className="flex flex-col gap-[16px]">
      <div>
        <div className="label-eyebrow text-[color:var(--text-faint)]">
          infrastructure
        </div>
        <h1 className="m-0 text-[length:var(--font-size-base)] font-medium text-[color:var(--text)]">
          Deployment Setup
        </h1>
      </div>

      {fetchError ? (
        <div className="rounded border border-[color:var(--danger)] bg-[color-mix(in_oklch,var(--danger)_10%,transparent)] p-[14px] text-[length:var(--font-size-sm)] text-[color:var(--danger)]">
          {fetchError}
        </div>
      ) : status ? (
        <div className="grid gap-[12px] [grid-template-columns:repeat(auto-fill,minmax(280px,1fr))]">
          <PostgresCard postgres={status.postgres} />
          <WorkerCard worker={status.worker} />
          <ClickHouseCard clickhouse={status.clickhouse} docker={status.docker} />
          <EtlProgressCard etl={etl} />
          <ApiKeysCard api_keys={status.api_keys} />
          <WebhookTargetCard webhook_target={status.webhook_target} />
        </div>
      ) : null}
    </div>
  );
}
