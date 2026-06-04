"use client";

import { useState } from "react";

import { Card } from "@/components/ui/card";
import { Tabs } from "@/components/ui/tabs";
import type { AnomalyItem } from "@/lib/api/anomalies";
import type { AlertRule } from "@/lib/api/alerts";

import { AlertRules } from "./alert-rules";
import { AlertsInbox } from "./alerts-inbox";

type TabValue = "inbox" | "rules";

export interface AlertsViewProps {
  incidents: ReadonlyArray<AnomalyItem>;
  rules: ReadonlyArray<AlertRule>;
}

export function AlertsView({ incidents, rules }: AlertsViewProps) {
  const [tab, setTab] = useState<TabValue>("inbox");
  return (
    <Card
      padding={false}
      eyebrow={`${incidents.length} open · ${rules.length} rules`}
      title="Alerts"
      action={
        <Tabs<TabValue>
          value={tab}
          onChange={setTab}
          items={[
            { value: "inbox", label: "Inbox", count: incidents.length },
            { value: "rules", label: "Rules", count: rules.length },
          ]}
        />
      }
    >
      {tab === "inbox" && <AlertsInbox incidents={incidents} />}
      {tab === "rules" && <AlertRules rules={rules} />}
    </Card>
  );
}
