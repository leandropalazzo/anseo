"use client";

import { useState } from "react";

import { SegControl } from "@/components/ui/seg-control";

import {
  OverallVisibilityView,
  type OverallVisibilityViewProps,
} from "./overall-visibility-view";
import { VisibilityView, type VisibilityViewProps } from "./visibility-view";

type Mode = "prompt" | "overall";

export interface VisibilityTabsProps {
  byPrompt: VisibilityViewProps;
  overall: OverallVisibilityViewProps;
}

/** Top-level switch between the per-prompt deep-dive and the all-prompts
 *  overview. Client-only so the toggle keeps both subtrees mounted state. */
export function VisibilityTabs({ byPrompt, overall }: VisibilityTabsProps) {
  const [mode, setMode] = useState<Mode>("prompt");
  return (
    <div className="flex flex-col gap-[12px]">
      <div className="flex">
        <SegControl<Mode>
          value={mode}
          onChange={setMode}
          options={[
            { value: "prompt", label: "By prompt" },
            { value: "overall", label: "Overall" },
          ]}
          ariaLabel="Visibility view"
        />
      </div>
      {mode === "prompt" ? (
        <VisibilityView {...byPrompt} />
      ) : (
        <OverallVisibilityView {...overall} />
      )}
    </div>
  );
}
