"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

import { useDeployment } from "@/components/shell/deployment-switch";
import { Logo } from "@/components/shell/logo";
import { ProjectSwitcher } from "@/components/shell/project-switcher";
import { Icon, ICON_DEFAULTS, type IconName } from "@/lib/icons";
import { isDevModeEnabled } from "@/lib/dev-mode";

interface NavItem {
  href: string;
  label: string;
  icon: IconName;
  shortcut?: string;
  badge?: string;
  disabled?: boolean;
}

interface NavGroup {
  id: string;
  label: string;
  items: ReadonlyArray<NavItem>;
}

/**
 * Source of truth for the in-app navigation. The Command Palette
 * reuses this same list so ⌘K nav targets always match the sidebar.
 *
 * IA (46.4): Monitor = core signal surfaces; Analyse = deep-dive analytics;
 * Operate = configuration and tooling.
 */
export const NAV_GROUPS: ReadonlyArray<NavGroup> = [
  {
    id: "monitor",
    label: "Monitor",
    items: [
      { href: "/", label: "Overview", icon: "Activity", shortcut: "G O" },
      { href: "/runs", label: "Runs", icon: "Box", shortcut: "G R" },
      { href: "/visibility", label: "Visibility", icon: "Trend", shortcut: "G V" },
      { href: "/citations", label: "Citations", icon: "Network", shortcut: "G C" },
      { href: "/competitors", label: "Competitors", icon: "Chart", shortcut: "G K" },
      { href: "/recommendations", label: "Recommendations", icon: "Sparkle", shortcut: "G D" },
      { href: "/alerts", label: "Alerts", icon: "Bell", shortcut: "G A" },
    ],
  },
  {
    id: "analyse",
    label: "Analyse",
    items: [
      { href: "/sentiment", label: "Sentiment", icon: "Activity", shortcut: "G T" },
      { href: "/hallucination", label: "Accuracy", icon: "Sparkle", shortcut: "G Y" },
      { href: "/audit", label: "Audit", icon: "Network", shortcut: "G U" },
      { href: "/crawlers", label: "Crawlers", icon: "Bot", shortcut: "G W" },
    ],
  },
  {
    id: "operate",
    label: "Operate",
    items: [
      { href: "/prompts", label: "Prompts", icon: "Yaml", shortcut: "G P" },
      { href: "/schedules", label: "Schedules", icon: "Calendar", shortcut: "G H" },
      { href: "/mcp", label: "MCP", icon: "Bot", shortcut: "G M" },
      { href: "/marketplace", label: "Marketplace", icon: "Layers", shortcut: "G B" },
      { href: "/settings", label: "Settings", icon: "Settings", shortcut: "G S" },
    ],
  },
];

function isActive(pathname: string, href: string): boolean {
  if (href === "/") return pathname === "/";
  return pathname === href || pathname.startsWith(`${href}/`);
}

function navItemTestId(href: string): string {
  return `nav-item-${href === "/" ? "overview" : href.slice(1).replaceAll("/", "-")}`;
}

// UX-DR120 — the Develop group + /dev item only appear when dev mode is on.
const DEV_GROUP: NavGroup = {
  id: "develop",
  label: "Develop",
  items: [{ href: "/dev", label: "Plugin Dev", icon: "Code", shortcut: "G X" }],
};

export function Sidebar() {
  const pathname = usePathname() ?? "/";
  const deployment = useDeployment();
  const groups = isDevModeEnabled() ? [...NAV_GROUPS, DEV_GROUP] : NAV_GROUPS;
  return (
    <aside
      className="sticky top-0 flex h-screen w-[224px] flex-shrink-0 flex-col overflow-hidden border-r border-[color:var(--border)] bg-[color:var(--bg-elev)]"
      data-testid="app-sidebar"
    >
      <div className="border-b border-[color:var(--hairline)] px-[14px] pb-[10px] pt-[14px]">
        <div className="flex min-w-0 items-center justify-between gap-[8px]">
          <Logo />
          <span className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
            v0.6.0 · GA
          </span>
        </div>
        <ProjectSwitcher deployment={deployment} />
      </div>

      <nav className="flex-1 overflow-auto px-[8px] py-[10px]" aria-label="Primary">
        {groups.map((g) => (
          <div key={g.id} className="mb-[14px]" data-testid={`nav-group-${g.id}`}>
            <div className="px-[8px] pb-[6px] pt-[4px] font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] uppercase tracking-[0.06em] text-[color:var(--text-faint)]">
              {g.label}
            </div>
            <div className="flex flex-col gap-[1px]">
              {g.items.map((n) => {
                const IconCmp = Icon[n.icon];
                const active = !n.disabled && isActive(pathname, n.href);
                const content = (
                  <>
                    {active && (
                      <span
                        aria-hidden
                        className="absolute -left-[8px] top-[6px] bottom-[6px] w-[2px] bg-[color:var(--accent)]"
                      />
                    )}
                    <span className="inline-flex min-w-0 items-center gap-[8px]">
                      <IconCmp
                        size={14}
                        strokeWidth={ICON_DEFAULTS.strokeWidth}
                        color={active ? "var(--accent)" : "currentColor"}
                      />
                      <span className="truncate">{n.label}</span>
                    </span>
                    <span className="shrink-0 font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                      {n.badge ?? n.shortcut}
                    </span>
                  </>
                );
                const className = [
                  "relative flex items-center justify-between gap-[8px] px-[8px] py-[6px]",
                  "whitespace-nowrap font-[family-name:var(--font-body)] text-[length:var(--font-size-sm)]",
                  active
                    ? "bg-[color:var(--bg-elev-2)] text-[color:var(--text)]"
                    : n.disabled
                      ? "cursor-not-allowed text-[color:var(--text-faint)] opacity-70"
                      : "text-[color:var(--text-muted)] hover:text-[color:var(--text)]",
                ].join(" ");

                if (n.disabled) {
                  return (
                    <div
                      key={n.href}
                      aria-disabled="true"
                      className={className}
                      data-testid={navItemTestId(n.href)}
                      title={`${n.label} is planned for an upcoming epic`}
                    >
                      {content}
                    </div>
                  );
                }

                return (
                  <Link
                    key={n.href}
                    href={n.href}
                    aria-current={active ? "page" : undefined}
                    className={className}
                    data-testid={navItemTestId(n.href)}
                  >
                    {content}
                  </Link>
                );
              })}
            </div>
          </div>
        ))}
      </nav>
    </aside>
  );
}
