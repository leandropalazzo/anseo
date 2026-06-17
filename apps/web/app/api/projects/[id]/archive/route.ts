// Server-side proxy for POST /v1/projects/:id/archive (Story 36.8 / 36.3).
// Settings calls this same-origin route so the server-only API key is attached.

import { NextResponse } from "next/server";
import { API_BASE_URL, setupHeaders } from "@/lib/api";

export async function POST(
  _req: Request,
  { params }: { params: Promise<{ id: string }> },
) {
  const { id } = await params;
  const r = await fetch(
    `${API_BASE_URL}/v1/projects/${encodeURIComponent(id)}/archive`,
    {
      method: "POST",
      headers: await setupHeaders(false),
      cache: "no-store",
    },
  );
  const out = await r.text();
  // Next.js 16 does not allow Response status 204 — map it to 200.
  const status = r.status === 204 ? 200 : r.status;
  return new NextResponse(out || "{}", {
    status,
    headers: { "Content-Type": "application/json" },
  });
}
