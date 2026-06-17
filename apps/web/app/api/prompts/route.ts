// Server-side proxy for GET/POST /v1/prompts (list + create). Client
// components cannot attach the server-only X-Anseo-API-Key header, so they
// call this same-origin route and we forward to the backend.

import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";
import { API_BASE_URL, setupHeaders } from "@/lib/api";

export async function GET() {
  const r = await fetch(`${API_BASE_URL}/v1/prompts`, {
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

export async function POST(req: NextRequest) {
  const body = await req.text();
  const r = await fetch(`${API_BASE_URL}/v1/prompts`, {
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
