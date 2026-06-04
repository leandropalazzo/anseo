// Server-side proxy for POST /v1/schedules/{id}/run (manual "run now").
// Runs on the Next server so the X-OpenGEO-API-Key header can be attached from
// server env; the browser never sees the operator key.

import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";
import { API_BASE_URL, setupHeaders } from "@/lib/api";

export async function POST(
  _req: NextRequest,
  ctx: { params: Promise<{ id: string }> },
) {
  const { id } = await ctx.params;
  const r = await fetch(
    `${API_BASE_URL}/v1/schedules/${encodeURIComponent(id)}/run`,
    {
      method: "POST",
      headers: await setupHeaders(false),
      cache: "no-store",
    },
  );
  const out = await r.text();
  return new NextResponse(out, {
    status: r.status,
    headers: { "Content-Type": "application/json" },
  });
}
