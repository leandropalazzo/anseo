// Auto-select the first active project on first visit (ADR-004).
//
// When no anseo_project cookie is present the Next.js proxy routes send no
// X-Anseo-Project header, so the backend's sole-active-project fallback (tier
// 3) fires. In multi-project deployments tier 3 returns 404, breaking every
// page that loads data on mount before client JS has a chance to run the
// project-switcher's auto-select logic.
//
// This middleware fixes the race by setting the cookie server-side during the
// initial navigation, before any React component mounts or fires a useEffect.
// Subsequent requests hit the fast path immediately (cookie present → skip).

import { NextRequest, NextResponse } from "next/server";
import { PROJECT_COOKIE, PROJECT_COOKIE_MAX_AGE } from "@/lib/projects";

interface ProjectsPayload {
  projects?: Array<{ name: string }>;
}

export async function middleware(req: NextRequest): Promise<NextResponse> {
  // Fast path: project already selected — nothing to do.
  if (req.cookies.get(PROJECT_COOKIE)?.value) {
    return NextResponse.next();
  }

  const apiBase =
    process.env.ANSEO_API_BASE_URL ?? "http://127.0.0.1:8080";
  const apiKey = process.env.ANSEO_API_KEY;
  const headers: Record<string, string> = {};
  if (apiKey) headers["X-Anseo-API-Key"] = apiKey;

  try {
    const r = await fetch(`${apiBase}/v1/projects`, {
      headers,
      cache: "no-store",
    });
    if (r.ok) {
      const data = (await r.json()) as ProjectsPayload;
      const first = data.projects?.[0]?.name;
      if (first) {
        const res = NextResponse.next();
        res.cookies.set(PROJECT_COOKIE, first, {
          path: "/",
          httpOnly: true,
          sameSite: "lax",
          maxAge: PROJECT_COOKIE_MAX_AGE,
        });
        return res;
      }
    }
  } catch {
    // API unreachable — let the request through; the page will show its own error.
  }

  return NextResponse.next();
}

export const config = {
  // Run only on page navigations. Skip API routes, Next.js internals, and
  // static assets so the proxy never fires on hot-reload or asset requests.
  matcher: [
    "/((?!api/|_next/|favicon\\.ico|.*\\.(?:png|jpg|jpeg|svg|ico|css|js|woff2?|ttf)).*)",
  ],
};
