"use client";

import { Cloud, Lock } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { ICON_DEFAULTS } from "@/lib/icons";

/**
 * Local mode → multi-user is cloud-only (single-user keychain phase-1).
 * The empty state offers an upsell + parity link to the deployment doc.
 */
export function TeamSection() {
  return (
    <Card eyebrow="local mode" title="Team & roles">
      <EmptyState
        icon={Lock}
        title="Single-user local deployment"
        hint="Multi-user / RBAC are cloud-only. Switch to Cloud in the chrome to invite teammates."
        action={
          <Button
            variant="primary"
            size="sm"
            leadingIcon={
              <Cloud size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />
            }
          >
            Upgrade to Cloud
          </Button>
        }
      />
    </Card>
  );
}
