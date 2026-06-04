// Server-side proxy for GET /v1/providers/openrouter/models — the live
// OpenRouter model catalog used to populate the create-schedule model
// dropdown. Client components can't attach the server-only API key header,
// so they call this same-origin route.

import { NextResponse } from "next/server";
import { API_BASE_URL, setupHeaders } from "@/lib/api";

export async function GET() {
  const r = await fetch(`${API_BASE_URL}/v1/providers/openrouter/models`, {
    method: "GET",
    headers: await setupHeaders(false),
    // Catalog changes rarely; let Next cache it briefly to avoid hammering
    // OpenRouter on every dialog open.
    next: { revalidate: 3600 },
  });
  const body = await r.text();
  return new NextResponse(body, {
    status: r.status,
    headers: { "Content-Type": "application/json" },
  });
}
