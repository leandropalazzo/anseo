// Server-side proxy for GET /v1/serve/status (Story 37.1 supervisor health).
// Client components (the topbar deployment indicator, the Deployment settings
// section) cannot attach the server-only X-OpenGEO-API-Key header, so they call
// this same-origin route and we forward to the backend. (Story 46.3.)

import { NextResponse } from "next/server";
import { API_BASE_URL, setupHeaders } from "@/lib/api";

export async function GET() {
  const r = await fetch(`${API_BASE_URL}/v1/serve/status`, {
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
