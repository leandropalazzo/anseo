import { Pill } from "@/components/ui/pill";
import type { MarketplacePlugin } from "@/lib/api";

// UX-DR90 — verified publishers get a prominent badge; unverified renders a
// distinct (not merely absent) chip so the trust state is never ambiguous.
export function VerifiedBadge({
  verified,
  signature_status,
}: Pick<MarketplacePlugin, "verified" | "signature_status">) {
  if (signature_status === "revoked") {
    return (
      <span data-testid="plugin-trust" data-trust="revoked">
        <Pill mono tone="danger">
          ✕ revoked
        </Pill>
      </span>
    );
  }
  if (verified) {
    return (
      <span data-testid="plugin-trust" data-trust="verified">
        <Pill mono tone="ok">
          ✓ verified publisher
        </Pill>
      </span>
    );
  }
  return (
    <span data-testid="plugin-trust" data-trust="unverified">
      <Pill mono tone="warn">
        ⚠ unverified
      </Pill>
    </span>
  );
}
