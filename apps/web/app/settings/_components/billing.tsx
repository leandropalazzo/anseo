"use client";

import { Box } from "lucide-react";

import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";

export function BillingSection() {
  return (
    <Card eyebrow="self-hosted · MIT" title="Billing">
      <EmptyState
        icon={Box}
        title="Anseo is free, self-hosted, MIT-licensed."
        hint="No billing in local mode. Provider API costs are billed by them directly."
      />
    </Card>
  );
}
