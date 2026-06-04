// Persist the operator-selected project (Story 36.8).
//
// The switcher POSTs the chosen project name here; we set it as an HTTP cookie
// so every subsequent SSR fetch and `app/api/*` proxy forwards the matching
// `X-OpenGEO-Project` header (resolved by name against the projects table).

import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";
import { PROJECT_COOKIE, PROJECT_COOKIE_MAX_AGE } from "@/lib/projects";

interface SelectBody {
  name?: unknown;
}

export async function POST(req: NextRequest) {
  let parsed: SelectBody;
  try {
    parsed = (await req.json()) as SelectBody;
  } catch {
    return NextResponse.json({ error: "invalid_body" }, { status: 400 });
  }
  const name = typeof parsed.name === "string" ? parsed.name.trim() : "";
  if (!name) {
    return NextResponse.json({ error: "name_required" }, { status: 400 });
  }
  const res = NextResponse.json({ ok: true, name });
  res.cookies.set(PROJECT_COOKIE, name, {
    path: "/",
    httpOnly: true,
    sameSite: "lax",
    maxAge: PROJECT_COOKIE_MAX_AGE,
  });
  return res;
}

export async function GET() {
  const { getSelectedProject } = await import("@/lib/projects");
  const name = (await getSelectedProject()) ?? null;
  return NextResponse.json({ name });
}
