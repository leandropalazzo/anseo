import Link from "next/link";
import { notFound } from "next/navigation";
import { ArrowLeft, ExternalLink } from "lucide-react";

import { Card } from "@/components/ui/card";
import { Pill } from "@/components/ui/pill";
import { CapabilityBlock } from "@/components/marketplace/capability-block";
import { InstallSheet } from "@/components/marketplace/install-sheet";
import { PluginDetailHeader } from "@/components/marketplace/plugin-detail-header";
import { fetchMarketplacePlugin } from "@/lib/api";

export const dynamic = "force-dynamic";

export default async function PluginDetailPage({
  params,
}: {
  params: Promise<{ slug: string }>;
}) {
  const { slug } = await params;
  const plugin = await fetchMarketplacePlugin(decodeURIComponent(slug));
  if (!plugin) notFound();

  return (
    <section
      data-testid="plugin-detail-page"
      className="flex flex-col gap-[12px]"
    >
      <div className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
        <Link
          href="/marketplace"
          className="inline-flex items-center gap-[5px] border border-[color:var(--border)] px-[8px] py-[4px] text-[color:var(--text-muted)] hover:text-[color:var(--text)]"
        >
          <ArrowLeft size={11} strokeWidth={1.5} /> Marketplace
        </Link>
      </div>

      <PluginDetailHeader plugin={plugin} />

      <Card>
        <CapabilityBlock capabilities={plugin.capabilities} />
      </Card>

      <Card>
        <div className="flex flex-col gap-[8px]">
          <div className="label-eyebrow text-[color:var(--text-faint)]">
            install
          </div>
          {plugin.installed ? (
            <div
              data-testid="plugin-installed-state"
              className="font-[family-name:var(--font-mono)] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]"
            >
              Installed{" "}
              <Pill mono tone="ok">
                v{plugin.installed_version ?? plugin.version}
              </Pill>
              {plugin.update_available && (
                <>
                  {" "}
                  — update to{" "}
                  <Pill mono tone="info">
                    v{plugin.version}
                  </Pill>{" "}
                  available
                </>
              )}
            </div>
          ) : (
            <div className="flex flex-col gap-[8px]">
              <InstallSheet plugin={plugin} />
              <div className="text-[length:var(--font-size-xs)] text-[color:var(--text-faint)]">
                Or from the CLI:{" "}
                <code className="font-[family-name:var(--font-mono)]">
                  ogeo plugin install {plugin.slug}@{plugin.version}
                </code>
              </div>
            </div>
          )}
          <a
            href={plugin.homepage}
            target="_blank"
            rel="noreferrer noopener"
            className="inline-flex w-fit items-center gap-[5px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)] hover:text-[color:var(--text)]"
          >
            Homepage <ExternalLink size={11} strokeWidth={1.5} />
          </a>
        </div>
      </Card>
    </section>
  );
}
