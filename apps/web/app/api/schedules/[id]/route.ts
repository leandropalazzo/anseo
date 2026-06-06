// Server-side proxy for PUT/DELETE /v1/schedules/{id} (pause/resume + delete).
// Runs on the Next server so the X-Anseo-API-Key header can be attached from
// server env; the browser never sees the operator key.

import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";
import { API_BASE_URL, setupHeaders } from "@/lib/api";

export async function PUT(
  req: NextRequest,
  ctx: { params: Promise<{ id: string }> },
) {
  const { id } = await ctx.params;
  const body = await req.text();
  const r = await fetch(`${API_BASE_URL}/v1/schedules/${encodeURIComponent(id)}`, {
    method: "PUT",
    headers: await setupHeaders(true),
    body,
    cache: "no-store",
  });
  const out = await r.text();
  return new NextResponse(out, {
    status: r.status,
    headers: { "Content-Type": "application/json" },
  });
}

export async function DELETE(
  _req: NextRequest,
  ctx: { params: Promise<{ id: string }> },
) {
  const { id } = await ctx.params;
  const r = await fetch(`${API_BASE_URL}/v1/schedules/${encodeURIComponent(id)}`, {
    method: "DELETE",
    headers: await setupHeaders(false),
    cache: "no-store",
  });
  if (r.status === 204) {
    return new NextResponse(null, { status: 204 });
  }
  const out = await r.text();
  return new NextResponse(out, {
    status: r.status,
    headers: { "Content-Type": "application/json" },
  });
}
