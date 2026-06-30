// Persist the operator-selected project (Story 36.8).
//
// The switcher POSTs the chosen project name here; we validate it against the
// live backend project list before setting the cookie so a stale or fabricated
// name can never enter the cookie and cause downstream 404s.

import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";
import { PROJECT_COOKIE, PROJECT_COOKIE_MAX_AGE } from "@/lib/projects";
import { API_BASE_URL } from "@/lib/api/_client";

interface SelectBody {
  name?: unknown;
}

interface ProjectsPayload {
  projects?: Array<{ name: string }>;
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

  // Validate against the live project list before persisting.
  const apiKey = process.env.ANSEO_API_KEY;
  if (apiKey) {
    try {
      const r = await fetch(`${API_BASE_URL}/v1/projects`, {
        headers: { "X-Anseo-API-Key": apiKey },
        cache: "no-store",
      });
      if (r.ok) {
        const data = (await r.json()) as ProjectsPayload;
        const known = data.projects?.map((p) => p.name) ?? [];
        if (!known.includes(name)) {
          return NextResponse.json(
            { error: "project_not_found", known },
            { status: 404 },
          );
        }
      }
    } catch {
      // Backend unreachable — allow the set so the UI isn't broken by a
      // transient outage; baseHeaders will re-validate on the next request.
    }
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
