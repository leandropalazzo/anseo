// Story 41.3 — server-side proxy for POST /v1/plugins/install so the operator
// API key is attached from server env (client-triggered Install button). The
// browser must never call the API directly: it has no key and the API base URL
// is a server-internal address.

import { NextResponse } from "next/server";
import { API_BASE_URL, setupHeaders } from "@/lib/api";

export async function POST(req: Request) {
  const body = await req.text();
  const r = await fetch(`${API_BASE_URL}/v1/plugins/install`, {
    method: "POST",
    headers: await setupHeaders(true),
    body,
    cache: "no-store",
  });
  const out = await r.text();
  return new NextResponse(out, {
    status: r.status === 204 ? 200 : r.status,
    headers: { "Content-Type": "application/json" },
  });
}
