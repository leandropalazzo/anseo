// Demo-data contract (Story 30-1).
//
// `IS_DEMO` is the single switch that turns mock fall-through on. When the live
// API returns no rows, surfaces should render an EmptyState — UNLESS the
// operator launched in demo mode (`OGEO_DEMO=1`), in which case mock data is
// shown alongside a visible <DemoBadge/>. The guard test in `lib/api.test.ts`
// enforces that mock imports under `app/**` only happen behind `IS_DEMO`.

/** True when the dashboard is launched in demo mode (`OGEO_DEMO=1`). */
export const IS_DEMO = process.env.OGEO_DEMO === "1";

export interface DemoOrEmptyResult<T> {
  data: T[];
  isDemo: boolean;
  isEmpty: boolean;
}

/**
 * Resolve a list against the demo-data contract.
 *
 * - Live data is non-empty → return it as-is (`isDemo:false`, `isEmpty:false`).
 * - Live data is empty + demo mode → return `mockFactory()` with `isDemo:true`.
 * - Live data is empty + not demo → return `[]` with `isEmpty:true` so the
 *   caller renders an <EmptyState/>.
 */
export function demoOrEmpty<T>(
  live: T[],
  mockFactory: () => T[],
): DemoOrEmptyResult<T> {
  if (live.length > 0) {
    return { data: live, isDemo: false, isEmpty: false };
  }
  return {
    data: IS_DEMO ? mockFactory() : [],
    isDemo: IS_DEMO,
    isEmpty: !IS_DEMO,
  };
}
