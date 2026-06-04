"use client";

import { useState, type ReactNode } from "react";

import { Tabs } from "@/components/ui/tabs";

type TabKey = "response" | "mentions" | "citations" | "raw" | "provenance";

export interface RunDetailTabsProps {
  responseSlot: ReactNode;
  mentionsSlot: ReactNode;
  citationsSlot: ReactNode;
  rawSlot: ReactNode;
  provenanceSlot: ReactNode;
}

export function RunDetailTabs({
  responseSlot,
  mentionsSlot,
  citationsSlot,
  rawSlot,
  provenanceSlot,
}: RunDetailTabsProps) {
  const [tab, setTab] = useState<TabKey>("response");
  return (
    <>
      <Tabs<TabKey>
        value={tab}
        onChange={setTab}
        items={[
          { value: "response", label: "Responses" },
          { value: "mentions", label: "Mentions" },
          { value: "citations", label: "Citations" },
          { value: "raw", label: "Raw JSON" },
          { value: "provenance", label: "Provenance" },
        ]}
      />
      <div className="mt-[12px]">
        {tab === "response" && responseSlot}
        {tab === "mentions" && mentionsSlot}
        {tab === "citations" && citationsSlot}
        {tab === "raw" && rawSlot}
        {tab === "provenance" && provenanceSlot}
      </div>
    </>
  );
}
