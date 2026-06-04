"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";
import { Bot, Box, Cloud, Database, FolderTree, Lock, Search, Sparkles, Tag } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { ICON_DEFAULTS } from "@/lib/icons";
import { ONBOARDED_FLAG } from "@/lib/mock-ops";

import { BillingSection } from "./_components/billing";
import { BrandSection } from "./_components/brand";
import { ProjectsSection } from "./_components/projects";
import { DeploySection } from "./_components/deploy";
import { ExtractorsSection } from "./_components/extractors";
import { PrivacySection } from "./_components/privacy";
import { ProvidersSection } from "./_components/providers";
import { TeamSection } from "./_components/team";

type SectionId =
  | "projects"
  | "providers"
  | "brand"
  | "privacy"
  | "deploy"
  | "extract"
  | "team"
  | "billing";

interface SectionMeta {
  id: SectionId;
  label: string;
  icon: LucideIcon;
}

const SECTIONS: ReadonlyArray<SectionMeta> = [
  { id: "projects",  label: "Projects",            icon: FolderTree },
  { id: "providers", label: "Providers & keys", icon: Database },
  { id: "brand",     label: "Brand & competitors", icon: Tag },
  { id: "privacy",   label: "Privacy posture",  icon: Lock },
  { id: "deploy",    label: "Deployment",       icon: Cloud },
  { id: "extract",   label: "Extractors",       icon: Search },
  { id: "team",      label: "Team & roles",     icon: Bot },
  { id: "billing",   label: "Billing",          icon: Box },
];

export default function SettingsPage() {
  const router = useRouter();
  const [section, setSection] = useState<SectionId>("projects");

  const rerunOnboarding = () => {
    try {
      window.localStorage.removeItem(ONBOARDED_FLAG);
    } catch {
      /* localStorage unavailable (Safari private, quota) — proceed anyway */
    }
    router.push("/onboarding");
  };

  return (
    <section
      data-testid="settings-page"
      className="grid gap-[12px] [grid-template-columns:200px_1fr]"
    >
      <div className="flex flex-col gap-[12px]">
        <Card padding={false} title="Settings">
          <div className="flex flex-col">
            {SECTIONS.map((s) => {
              const active = section === s.id;
              const Icon = s.icon;
              return (
                <button
                  key={s.id}
                  type="button"
                  onClick={() => setSection(s.id)}
                  className={[
                    "flex cursor-pointer appearance-none items-center gap-[8px] border-0 border-b border-[color:var(--hairline)] px-[12px] py-[8px] text-left text-[length:var(--font-size-sm)]",
                    active
                      ? "bg-[color:var(--bg-elev-2)] text-[color:var(--text)]"
                      : "bg-transparent text-[color:var(--text-muted)] hover:text-[color:var(--text)]",
                  ].join(" ")}
                  style={{
                    borderLeft: active
                      ? "2px solid var(--accent)"
                      : "2px solid transparent",
                  }}
                  data-testid={`settings-section-${s.id}`}
                >
                  <Icon size={12} strokeWidth={ICON_DEFAULTS.strokeWidth} />
                  {s.label}
                </button>
              );
            })}
          </div>
        </Card>
        <Card eyebrow="first-run flow" title="Onboarding">
          <p className="m-0 text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
            Walk through the five-step setup again (init, providers, brand,
            first run, schedule).
          </p>
          <div className="mt-[10px]">
            <Button
              variant="secondary"
              size="sm"
              onClick={rerunOnboarding}
              leadingIcon={
                <Sparkles
                  size={11}
                  strokeWidth={ICON_DEFAULTS.strokeWidth}
                />
              }
              data-testid="rerun-onboarding"
            >
              Re-run onboarding
            </Button>
          </div>
        </Card>
      </div>
      <div>
        {section === "projects" && <ProjectsSection />}
        {section === "providers" && <ProvidersSection />}
        {section === "brand" && <BrandSection />}
        {section === "privacy" && <PrivacySection />}
        {section === "deploy" && <DeploySection />}
        {section === "extract" && <ExtractorsSection />}
        {section === "team" && <TeamSection />}
        {section === "billing" && <BillingSection />}
      </div>
    </section>
  );
}
