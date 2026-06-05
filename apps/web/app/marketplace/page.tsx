import { fetchMarketplacePlugins, type MarketplacePlugin } from "@/lib/api";
import { PageHeader } from "@/components/ui/page-header";

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
      <PageHeader
        title="Marketplace"
        description={<>Discover and inspect plugins. Installs run from the <code className="font-[family-name:var(--font-mono)]">ogeo</code> CLI.</>}
      />
      <MarketplaceBrowse plugins={plugins} />
    </section>
  );
}
