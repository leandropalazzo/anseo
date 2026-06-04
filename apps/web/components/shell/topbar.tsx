"use client";

import { usePathname, useRouter } from "next/navigation";
import { useEffect, useState, useTransition, type ReactNode } from "react";

import { ThemeToggle } from "@/app/theme-toggle";
import { DeploymentSwitch } from "@/components/shell/deployment-switch";
import { KBD } from "@/components/ui/kbd";
import { fetchAnomalies } from "@/lib/api/anomalies";
import { Icon, ICON_DEFAULTS, type LucideIcon } from "@/lib/icons";

/** Maps the first path segment to the human breadcrumb label. */
const ROUTE_TITLES: Readonly<Record<string, string>> = {
  "": "Overview",
  runs: "Prompt Runs",
  visibility: "Visibility",
  citations: "Citations",
  competitors: "Competitors",
  prompts: "Prompts",
  alerts: "Alerts",
  schedules: "Schedules",
  mcp: "MCP Server",
  settings: "Settings",
  onboarding: "Get started",
  "%5Fdesign-sandbox": "Design sandbox",
  _design_sandbox: "Design sandbox",
};

function titleForPath(pathname: string): string {
  if (pathname === "/" || pathname === "") return ROUTE_TITLES[""];
  const seg = pathname.split("/").filter(Boolean)[0] ?? "";
  return ROUTE_TITLES[seg] ?? seg.charAt(0).toUpperCase() + seg.slice(1);
}

interface TopbarProps {
  /** Opens the Command Palette — wired by AppShell. */
  onOpenPalette: () => void;
}

export function Topbar({ onOpenPalette }: TopbarProps) {
  const pathname = usePathname() ?? "/";
  const router = useRouter();
  const title = titleForPath(pathname);
  const [isSyncing, startSync] = useTransition();
  const [alertCount, setAlertCount] = useState<number | undefined>(undefined);
  const [project, setProject] = useState<string>("opengeo");

  // Live unread count for the bell: open anomalies in the last 7d. Best-effort
  // — a fetch failure simply leaves the badge off.
  useEffect(() => {
    let active = true;
    fetchAnomalies("7d")
      .then((items) => {
        if (active) setAlertCount(items.length);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [pathname]);

  // Reflect the operator-selected project in the breadcrumb (Story 36.8).
  // Best-effort: a fetch failure leaves the default label.
  useEffect(() => {
    let active = true;
    fetch("/api/projects/select", { cache: "no-store" })
      .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
      .then((j: { name?: string | null }) => {
        if (active && j.name) setProject(j.name);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [pathname]);

  return (
    <header
      data-testid="app-topbar"
      className="sticky top-0 z-10 border-b border-[color:var(--border)] backdrop-blur-[8px]"
      style={{
        background: "color-mix(in oklch, var(--bg-elev) 92%, transparent)",
      }}
    >
      <div className="flex items-center justify-between gap-[12px] px-[18px] py-[8px]">
        <Breadcrumbs project={project} title={title} />
        <div className="flex items-center gap-[8px]">
          <DeploymentSwitch />
          <SearchTrigger onClick={onOpenPalette} />
          <IconBtn
            icon={Icon.Refresh}
            tooltip="Re-sync"
            onClick={() => startSync(() => router.refresh())}
            busy={isSyncing}
          />
          <IconBtn
            icon={Icon.Bell}
            tooltip="Notifications"
            onClick={() => router.push("/alerts")}
            badge={alertCount && alertCount > 0 ? alertCount : undefined}
          />
          <ThemeToggle />
          <UserAvatar initials="DA" />
        </div>
      </div>
    </header>
  );
}

function Breadcrumbs({ project, title }: { project: string; title: string }) {
  return (
    <div className="flex min-w-0 items-center gap-[8px]">
      <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
        {project}
      </span>
      <Icon.ChevronRight
        size={11}
        strokeWidth={ICON_DEFAULTS.strokeWidth}
        className="text-[color:var(--text-faint)]"
      />
      <span className="whitespace-nowrap font-[family-name:var(--font-display)] text-[16px] font-medium tracking-[var(--display-tracking)] text-[color:var(--text)]">
        {title}
      </span>
    </div>
  );
}

function SearchTrigger({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      data-testid="search-trigger"
      className="inline-flex min-w-[200px] cursor-pointer items-center gap-[8px] border border-[color:var(--border)] bg-[color:var(--bg-sunken)] px-[8px] py-[4px] font-[family-name:var(--font-body)] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]"
    >
      <Icon.Search
        size={12}
        strokeWidth={ICON_DEFAULTS.strokeWidth}
        className="text-[color:var(--text-faint)]"
      />
      <span className="flex-1 text-left">Search or run a command…</span>
      <KBD>⌘K</KBD>
    </button>
  );
}

function IconBtn({
  icon: IconCmp,
  tooltip,
  badge,
  onClick,
  busy,
}: {
  icon: LucideIcon;
  tooltip: string;
  badge?: number;
  onClick?: () => void;
  busy?: boolean;
}) {
  return (
    <button
      type="button"
      title={tooltip}
      aria-label={tooltip}
      onClick={onClick}
      disabled={busy}
      className="relative inline-flex cursor-pointer items-center justify-center border border-[color:var(--border)] bg-transparent p-[5px] text-[color:var(--text-muted)] hover:text-[color:var(--text)] disabled:cursor-default disabled:opacity-60"
    >
      <IconCmp
        size={13}
        strokeWidth={ICON_DEFAULTS.strokeWidth}
        className={busy ? "animate-spin" : undefined}
      />
      {badge != null && (
        <span
          aria-label={`${badge} unread`}
          className="absolute -right-[4px] -top-[4px] inline-flex h-[14px] min-w-[14px] items-center justify-center bg-[color:var(--danger)] px-[3px] font-[family-name:var(--font-mono)] text-[9px] text-[color:var(--bg)]"
        >
          {badge}
        </span>
      )}
    </button>
  );
}

function UserAvatar({ initials }: { initials: string }): ReactNode {
  return (
    <div
      aria-label={`User: ${initials}`}
      className="inline-flex h-[24px] w-[24px] items-center justify-center border border-[color:var(--border-strong)] font-[family-name:var(--font-mono)] text-[10px] font-semibold text-[color:var(--accent-ink)]"
      style={{
        background:
          "linear-gradient(135deg, var(--accent), color-mix(in oklch, var(--accent) 60%, var(--info)))",
      }}
    >
      {initials}
    </div>
  );
}
