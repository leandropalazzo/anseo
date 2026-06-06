/**
 * Diagnostics logger for the `@anseo/observe` SDK.
 *
 * Mirrors the Python reference (`anseo_observe`), which emits on a logger named
 * `"anseo"`. There is no logging framework dependency: the SDK writes to a
 * pluggable sink that defaults to the console, and `debug`-level output is gated
 * behind the `DEBUG` env var (matching the core spec's `DEBUG=anseo` switch).
 *
 * Best-effort delivery means the SDK never throws into the host app — every
 * transport/server failure is funnelled through here instead.
 */

/** A minimal logging sink. The default uses `console`. */
export interface AnseoLogger {
  debug(message: string, ...args: unknown[]): void;
  warn(message: string, ...args: unknown[]): void;
}

/**
 * Returns true when DEBUG diagnostics should be emitted. Honours the same
 * `DEBUG` convention as the Python reference (`DEBUG=anseo`): any value that
 * names `anseo` (or the wildcard `*`) turns on debug logging. Reads
 * `process.env` defensively so the SDK still works where `process` is absent.
 */
function debugEnabled(): boolean {
  const env = (globalThis as { process?: { env?: Record<string, string | undefined> } })
    .process?.env;
  const flag = env?.DEBUG;
  if (!flag) return false;
  if (flag === "*" || flag === "1" || flag === "true") return true;
  return flag.split(/[\s,]+/).some((tok) => tok === "anseo" || tok === "anseo:*");
}

/** The default sink: warnings always print; debug prints only when enabled. */
export const defaultLogger: AnseoLogger = {
  debug(message: string, ...args: unknown[]): void {
    if (debugEnabled()) {
      console.debug(`anseo: ${message}`, ...args);
    }
  },
  warn(message: string, ...args: unknown[]): void {
    console.warn(`anseo: ${message}`, ...args);
  },
};
