// Server-side proxy for GET /v1/setup/status. Client components (e.g. the
// Settings → Providers key manager) cannot attach the server-only
// X-Anseo-API-Key header, so they call this same-origin route instead and
// we forward to the backend with the key read from server env.

import { NextResponse } from "next/server";
import { API_BASE_URL, setupHeaders } from "@/lib/api";

export async function GET() {
  const r = await fetch(`${API_BASE_URL}/v1/setup/status`, {
    method: "GET",
    headers: await setupHeaders(false),
    cache: "no-store",
  });
  const body = await r.text();
  return new NextResponse(body, {
    status: r.status === 204 ? 200 : r.status,
    headers: { "Content-Type": "application/json" },
  });
}
