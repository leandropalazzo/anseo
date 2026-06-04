import { fetchMarketplacePlugins, type MarketplacePlugin } from "@/lib/api";

import { MarketplaceBrowse } from "./_components/marketplace-browse";

export const dynamic = "force-dynamic";

export default async function MarketplacePage() {
  let plugins: MarketplacePlugin[] = [];
  try {
    plugins = await fetchMarketplacePlugins();
  } catch {
    plugins = [];
  }

  return (
    <section
      data-testid="marketplace-page"
      className="flex flex-col gap-[12px]"
    >
      <header>
        <h1 className="m-0 text-[length:22px] font-normal tracking-[var(--display-tracking)] text-[color:var(--text)]">
          Marketplace
        </h1>
        <p className="m-0 mt-[2px] text-[length:var(--font-size-sm)] text-[color:var(--text-muted)]">
          Discover and inspect plugins. Installs run from the{" "}
          <code className="font-[family-name:var(--font-mono)]">ogeo</code> CLI.
        </p>
      </header>
      <MarketplaceBrowse plugins={plugins} />
    </section>
  );
}
