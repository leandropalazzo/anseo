// Server-side proxy for GET/PUT /v1/setup/brand (DB-authoritative brand
// config). Client components cannot attach the server-only X-Anseo-API-Key
// header, so they call this same-origin route and we forward to the backend.

import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";
import { API_BASE_URL, setupHeaders } from "@/lib/api";

export async function GET() {
  const r = await fetch(`${API_BASE_URL}/v1/setup/brand`, {
    method: "GET",
    headers: await setupHeaders(false),
    cache: "no-store",
  });
  const body = await r.text();
  return new NextResponse(body, {
    status: r.status,
    headers: { "Content-Type": "application/json" },
  });
}

export async function PUT(req: NextRequest) {
  const body = await req.text();
  const r = await fetch(`${API_BASE_URL}/v1/setup/brand`, {
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
