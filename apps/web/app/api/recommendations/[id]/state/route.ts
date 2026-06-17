// Server-side proxy for PATCH /v1/recommendations/:id/state so the operator
// API key is attached from server env (client-triggered lifecycle actions:
// snooze / mark-acted / dismiss).

import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";
import { API_BASE_URL, setupHeaders } from "@/lib/api";

export async function PATCH(
  req: NextRequest,
  ctx: { params: Promise<{ id: string }> },
) {
  const { id } = await ctx.params;
  const body = await req.text();
  const r = await fetch(
    `${API_BASE_URL}/v1/recommendations/${encodeURIComponent(id)}/state`,
    {
      method: "PATCH",
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
