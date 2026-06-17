// Server-side proxy for POST /v1/setup/api-keys/{provider} (store a provider
// key). Runs on the Next server so the X-Anseo-API-Key header can be
// attached from server env; the browser never sees the operator key.

import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";
import { API_BASE_URL, setupHeaders } from "@/lib/api";

export async function POST(
  req: NextRequest,
  ctx: { params: Promise<{ provider: string }> },
) {
  const { provider } = await ctx.params;
  const body = await req.text();
  const r = await fetch(
    `${API_BASE_URL}/v1/setup/api-keys/${encodeURIComponent(provider)}`,
    {
      method: "POST",
      headers: await setupHeaders(true),
      body,
      cache: "no-store",
    },
  );
  const out = await r.text();
  return new NextResponse(out, {
    status: r.status === 204 ? 200 : r.status,
    headers: { "Content-Type": "application/json" },
  });
}
