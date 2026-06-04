// Server-side proxy for POST /v1/setup/api-keys/{provider}/revoke. See the
// sibling route.ts for why client mutations are proxied server-side.

import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";
import { API_BASE_URL, setupHeaders } from "@/lib/api";

export async function POST(
  _req: NextRequest,
  ctx: { params: Promise<{ provider: string }> },
) {
  const { provider } = await ctx.params;
  const r = await fetch(
    `${API_BASE_URL}/v1/setup/api-keys/${encodeURIComponent(provider)}/revoke`,
    {
      method: "POST",
      headers: await setupHeaders(false),
      cache: "no-store",
    },
  );
  if (r.status === 204) {
    return new NextResponse(null, { status: 204 });
  }
  const out = await r.text();
  return new NextResponse(out || "{}", {
    status: r.status,
    headers: { "Content-Type": "application/json" },
  });
}
