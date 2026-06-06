// Server-side proxy for POST /v1/prompts/suggest. Runs on the Next server so
// the X-Anseo-API-Key header can be attached from server env.

import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";
import { API_BASE_URL, setupHeaders } from "@/lib/api";

export async function POST(req: NextRequest) {
  const body = await req.text();
  const r = await fetch(`${API_BASE_URL}/v1/prompts/suggest`, {
    method: "POST",
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
