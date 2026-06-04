"use client";

import { useState } from "react";

import { Bar } from "@/components/charts/bar";
import { Sparkline } from "@/components/charts/sparkline";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { CodeBlock } from "@/components/ui/code-block";
import { EmptyState } from "@/components/ui/empty-state";
import { KBD } from "@/components/ui/kbd";
import { Pill, type PillTone } from "@/components/ui/pill";
import { ProviderDot } from "@/components/ui/provider-dot";
import { SegControl } from "@/components/ui/seg-control";
import { StatTile } from "@/components/ui/stat-tile";
import { Tabs } from "@/components/ui/tabs";
import { Icon, ICON_DEFAULTS } from "@/lib/icons";
import type { ProviderId } from "@/lib/provider-colors";

const TONES: ReadonlyArray<PillTone> = [
  "neutral",
  "ok",
  "warn",
  "danger",
  "info",
  "accent",
];

const PROVIDERS: ReadonlyArray<ProviderId> = [
  "openai",
  "anthropic",
  "gemini",
  "perplexity",
];

const SAMPLE_TREND = [12, 14, 13, 18, 22, 21, 24, 28, 27, 30, 33, 31, 35, 38];

type Theme = "dark" | "light";
type Tab = "primitives" | "charts" | "icons" | "shell";

function openCommandPalette() {
  // The AppShell owns the ⌘K listener at the root; dispatch a synthetic
  // keystroke so reviewers can pop the palette from a click without
  // duplicating state.
  if (typeof window === "undefined") return;
  window.dispatchEvent(
    new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }),
  );
}

function ShellPanel() {
  return (
    <div className="grid gap-[14px]">
      <Card title="Shell layout" eyebrow="shell · live">
        <p className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
          You are inside it. The sidebar (left) and topbar (above) are the
          AppShell rendered by <code>app/layout.tsx</code>. Active nav state is
          driven by <code>usePathname()</code>; the topbar breadcrumb reads the
          same.
        </p>
        <div className="mt-[14px] flex flex-wrap items-center gap-[8px]">
          <Button variant="primary" onClick={openCommandPalette}>
            Open command palette
          </Button>
          <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
            or press <KBD>⌘K</KBD> / <KBD>Ctrl K</KBD>
          </span>
        </div>
      </Card>
      <Card title="Deployment switch" eyebrow="shell · deployment">
        <p className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
          Toggle Local/Cloud in the topbar — your choice persists to
          <code> localStorage[&apos;opengeo:deployment&apos;]</code> and the
          sidebar&apos;s project switcher icon updates in sync.
        </p>
      </Card>
    </div>
  );
}

function Panel({ theme, children }: { theme: Theme; children: React.ReactNode }) {
  return (
    <div
      data-theme={theme}
      className="border border-[color:var(--border)] bg-[color:var(--bg)] p-[16px] text-[color:var(--text)]"
    >
      <div className="label-eyebrow mb-[12px] text-[color:var(--text-faint)]">
        Theme · {theme}
      </div>
      {children}
    </div>
  );
}

function SandboxTabs() {
  const [v, setV] = useState<"inbox" | "rules" | "schedules">("inbox");
  return (
    <Tabs<"inbox" | "rules" | "schedules">
      value={v}
      onChange={setV}
      items={[
        { value: "inbox", label: "Inbox", count: 3 },
        { value: "rules", label: "Rules", count: 8 },
        { value: "schedules", label: "Schedules", count: 2 },
      ]}
    />
  );
}

function PrimitivesPanel({ theme }: { theme: Theme }) {
  const [seg, setSeg] = useState<"local" | "cloud">("local");
  return (
    <Panel theme={theme}>
      <div className="grid gap-[14px]">
        <Card title="Pills" eyebrow="ui · pill">
          <div className="flex flex-wrap gap-[6px]">
            {TONES.map((t) => (
              <Pill key={t} tone={t}>
                {t}
              </Pill>
            ))}
            {TONES.map((t) => (
              <Pill key={`${t}-solid`} tone={t} solid>
                {t}
              </Pill>
            ))}
          </div>
        </Card>

        <Card title="Provider dots" eyebrow="ui · provider-dot">
          <div className="flex flex-wrap items-center gap-[14px]">
            {PROVIDERS.map((p) => (
              <ProviderDot key={p} provider={p} withLabel />
            ))}
            {PROVIDERS.map((p) => (
              <ProviderDot key={`${p}-dim`} provider={p} dim />
            ))}
          </div>
        </Card>

        <Card title="KBD + Buttons" eyebrow="ui · kbd / button">
          <div className="flex flex-wrap items-center gap-[8px]">
            <KBD>⌘K</KBD>
            <KBD>esc</KBD>
            <KBD>G O</KBD>
            <Button variant="primary">Primary</Button>
            <Button variant="secondary">Secondary</Button>
            <Button variant="ghost">Ghost</Button>
            <Button variant="danger">Danger</Button>
            <Button
              size="sm"
              leadingIcon={<Icon.Plus size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />}
            >
              Small
            </Button>
          </div>
        </Card>

        <Card title="Stat tiles" eyebrow="ui · stat-tile">
          <div className="grid grid-cols-1 gap-[10px] md:grid-cols-3">
            <StatTile
              label="Mentions · 24h"
              value="1,284"
              delta="+12%"
              deltaTone="ok"
              sparkline={<Sparkline points={SAMPLE_TREND} />}
            />
            <StatTile
              label="Median rank"
              value="3.2"
              delta="-0.4"
              deltaTone="warn"
              mono
            />
            <StatTile
              label="Errors"
              value="7"
              delta="+3"
              deltaTone="danger"
              big
              mono
            />
          </div>
        </Card>

        <Card title="Tabs + SegControl" eyebrow="ui · tabs / seg-control">
          <SandboxTabs />
          <div className="mt-[14px]">
            <SegControl
              value={seg}
              onChange={setSeg}
              ariaLabel="Deployment"
              options={[
                {
                  value: "local",
                  label: "Local",
                  icon: <Icon.Server size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />,
                },
                {
                  value: "cloud",
                  label: "Cloud",
                  icon: <Icon.Cloud size={11} strokeWidth={ICON_DEFAULTS.strokeWidth} />,
                },
              ]}
            />
          </div>
        </Card>

        <Card title="Code block" eyebrow="ui · code-block">
          <CodeBlock
            lang="bash"
            code={`ogeo prompt run --prompt vector-db --provider openai
ogeo report generate --window 7d --format markdown`}
          />
        </Card>

        <Card title="Empty state" eyebrow="ui · empty-state">
          <EmptyState
            icon={Icon.Box}
            title="No prompts yet"
            hint="Run `ogeo prompt new` to create your first prompt."
            action={<Button variant="primary">New prompt</Button>}
          />
        </Card>

        <Card title="Accent rail card" eyebrow="ui · card" accent>
          <div className="text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
            Card with the optional 2px accent rail enabled.
          </div>
        </Card>
      </div>
    </Panel>
  );
}

function ChartsPanel({ theme }: { theme: Theme }) {
  return (
    <Panel theme={theme}>
      <div className="grid gap-[14px]">
        <Card title="Sparkline" eyebrow="charts · sparkline">
          <div className="flex flex-wrap items-center gap-[20px]">
            <Sparkline points={SAMPLE_TREND} />
            <Sparkline points={SAMPLE_TREND} color="var(--ok)" />
            <Sparkline points={SAMPLE_TREND} color="var(--danger)" fill={false} />
          </div>
        </Card>

        <Card title="Bar" eyebrow="charts · bar">
          <div className="grid gap-[10px]">
            {[0.2, 0.55, 0.82, 1].map((v, i) => (
              <div key={i} className="flex items-center gap-[10px]">
                <span className="w-[40px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                  {Math.round(v * 100)}%
                </span>
                <div className="flex-1">
                  <Bar value={v} ariaLabel={`Sample ${i}`} />
                </div>
              </div>
            ))}
          </div>
        </Card>
      </div>
    </Panel>
  );
}

function IconsPanel({ theme }: { theme: Theme }) {
  const entries = Object.entries(Icon) as ReadonlyArray<
    readonly [keyof typeof Icon, (typeof Icon)[keyof typeof Icon]]
  >;
  return (
    <Panel theme={theme}>
      <Card title="Icon registry" eyebrow="lib · icons">
        <div className="grid grid-cols-3 gap-[6px] md:grid-cols-6">
          {entries.map(([name, Cmp]) => (
            <div
              key={name}
              className="flex items-center gap-[8px] border border-[color:var(--hairline)] bg-[color:var(--bg-elev-2)] px-[8px] py-[6px]"
            >
              <Cmp size={ICON_DEFAULTS.size} strokeWidth={ICON_DEFAULTS.strokeWidth} />
              <span className="truncate font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-muted)]">
                {name}
              </span>
            </div>
          ))}
        </div>
      </Card>
    </Panel>
  );
}

export default function DesignSandboxPage() {
  const [tab, setTab] = useState<Tab>("primitives");
  return (
    <div className="grid gap-[20px]">
      <div>
        <div className="label-eyebrow text-[color:var(--text-faint)]">
          chunk ux-a · sandbox
        </div>
        <h1 className="mt-[4px] text-[length:24px] font-medium tracking-[var(--display-tracking)] text-[color:var(--text)]">
          Foundation primitives
        </h1>
        <p className="mt-[6px] max-w-[640px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
          Every primitive rendered in dark + light, side by side. Use this
          page during visual review to verify token contrast and component
          composition.
        </p>
      </div>

      <Tabs<Tab>
        value={tab}
        onChange={setTab}
        items={[
          { value: "primitives", label: "Primitives" },
          { value: "charts", label: "Charts" },
          { value: "icons", label: "Icons" },
          { value: "shell", label: "Shell" },
        ]}
      />

      <div className="grid grid-cols-1 gap-[14px] xl:grid-cols-2">
        {tab === "primitives" && (
          <>
            <PrimitivesPanel theme="dark" />
            <PrimitivesPanel theme="light" />
          </>
        )}
        {tab === "charts" && (
          <>
            <ChartsPanel theme="dark" />
            <ChartsPanel theme="light" />
          </>
        )}
        {tab === "icons" && (
          <>
            <IconsPanel theme="dark" />
            <IconsPanel theme="light" />
          </>
        )}
        {tab === "shell" && <ShellPanel />}
      </div>
    </div>
  );
}
