"use client";

import { usePathname, useRouter } from "next/navigation";
import { useTransition } from "react";

import { SegControl } from "@/components/ui/seg-control";
import type { AnalyticsPeriod } from "@/lib/api";

/**
 * 7d / 30d window toggle. Re-fetches server-side by navigating to
 * `/analytics?period=...` so the operator API key stays on the server (the page
 * is a server component that reads `searchParams.period`). Uses a transition so
 * the toggle stays responsive while the server re-renders.
 */
export function PeriodToggle({ value }: { value: AnalyticsPeriod }) {
  const router = useRouter();
  const pathname = usePathname() ?? "/analytics";
  const [pending, startTransition] = useTransition();

  return (
    <div data-testid="analytics-period-toggle" aria-busy={pending}>
      <SegControl<AnalyticsPeriod>
        ariaLabel="Analytics time window"
        value={value}
        onChange={(next) => {
          startTransition(() => {
            router.push(`${pathname}?period=${next}`);
          });
        }}
        options={[
          { value: "7d", label: "7D" },
          { value: "30d", label: "30D" },
        ]}
      />
    </div>
  );
}
