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
  return new NextResponse(out || "{}", {
    status: r.status,
    headers: { "Content-Type": "application/json" },
  });
}
