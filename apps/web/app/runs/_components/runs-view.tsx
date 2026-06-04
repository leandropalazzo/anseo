"use client";

import { useMemo, useState } from "react";
import { Download } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Tabs } from "@/components/ui/tabs";
import type { MockRun } from "@/lib/mock";
import {
  providerRunIdentity,
  resolveProviderIdentity,
  resolveRunFilterProviderId,
} from "@/lib/provider-colors";

import { ProviderFilterChips } from "./provider-filter-chips";
import { RunsTable } from "./runs-table";

type Filter = "all" | "failed" | "anomalies";

export interface RunsViewProps {
  runs: ReadonlyArray<MockRun>;
}

/**
 * Client wrapper that owns the tabs + provider-chip filter state. The
 * server page hands it an immutable run list (live + mock-enriched).
 *
 * "Anomalies" filter is a stand-in until the anomaly engine ships — for
 * now it surfaces failed runs and runs with rank >= 6 (the same threshold
 * that drives the danger pill in <ProviderRanks>).
 */
export function RunsView({ runs }: RunsViewProps) {
  const [filter, setFilter] = useState<Filter>("all");
  const [providers, setProviders] = useState<ReadonlySet<string>>(
    new Set(),
  );

  const providerOptions = useMemo(() => {
    const seen = new Set<string>();
    for (const r of runs) {
      const filterId = resolveRunFilterProviderId(
        providerRunIdentity(r.provider, r.provider_model_version),
      );
      if (filterId) seen.add(filterId);
    }
    return [...seen].sort((a, b) =>
      resolveProviderIdentity(a).label.localeCompare(
        resolveProviderIdentity(b).label,
      ),
    );
  }, [runs]);

  const filtered = useMemo(() => {
    let list = runs;
    if (filter === "failed") list = list.filter((r) => r.status === "failed");
    if (filter === "anomalies")
      list = list.filter(
        (r) => r.status === "failed" || (r.brand_rank ?? 0) >= 6,
      );
    if (providers.size > 0) {
      list = list.filter((r) => {
        const filterId = resolveRunFilterProviderId(
          providerRunIdentity(r.provider, r.provider_model_version),
        );
        return filterId !== null && providers.has(filterId);
      });
    }
    return list;
  }, [runs, filter, providers]);

  const counts = useMemo(
    () => ({
      all: runs.length,
      failed: runs.filter((r) => r.status === "failed").length,
      anomalies: runs.filter(
        (r) => r.status === "failed" || (r.brand_rank ?? 0) >= 6,
      ).length,
    }),
    [runs],
  );

  const onToggleProvider = (p: string | null) => {
    if (p === null) {
      setProviders(new Set());
      return;
    }
    const next = new Set(providers);
    if (next.has(p)) next.delete(p);
    else next.add(p);
    setProviders(next);
  };

  return (
    <Card
      padding={false}
      eyebrow="prompt_runs · postgres"
      title={`${filtered.length} runs`}
      action={
        <Tabs<Filter>
          value={filter}
          onChange={setFilter}
          items={[
            { value: "all", label: "All", count: counts.all },
            { value: "failed", label: "Failed", count: counts.failed },
            { value: "anomalies", label: "Anomalies", count: counts.anomalies },
          ]}
        />
      }
    >
      <ProviderFilterChips
        selected={providers}
        providers={providerOptions}
        onToggle={onToggleProvider}
      />
      <div className="flex items-center justify-end px-[14px] py-[6px]">
        <Button
          variant="ghost"
          size="sm"
          leadingIcon={<Download size={11} strokeWidth={1.5} />}
        >
          Export
        </Button>
      </div>
      <RunsTable runs={filtered} />
    </Card>
  );
}
