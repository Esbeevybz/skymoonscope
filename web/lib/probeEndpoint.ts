/**
 * probeEndpoint — lightweight HTTP liveness probe with a hard timeout.
 *
 * Extracted from pages/settings.tsx so it can be unit-tested in isolation
 * (Node's test runner cannot execute React component files).
 *
 * Design decisions
 * ----------------
 * • Uses `AbortController` + `setTimeout` rather than `AbortSignal.timeout()`
 *   for broader runtime compatibility (Node < 17.3, older Safari).
 * • `clearTimeout` is always called in the `finally` block so the timer
 *   cannot leak if the caller disposes of the reference early.
 * • Any HTTP response (even 4xx/5xx) proves the host is reachable; only
 *   network-level failures and timeouts count as "fail".
 */

export interface ProbeResult {
  status: 'ok' | 'fail';
  message: string;
  latencyMs?: number;
}

/**
 * Probe `url` with a single HTTP request. Resolves (never rejects) with a
 * `ProbeResult` describing whether the endpoint answered within `timeoutMs`.
 *
 * @param url       Full URL to probe.
 * @param body      JSON-serialisable body to POST, or `null` for a GET.
 * @param timeoutMs Hard deadline in milliseconds. Must be ≥ 1.
 * @param signal    Optional external `AbortSignal` that cancels the probe
 *                  before `timeoutMs` elapses (e.g. component unmount).
 */
export async function probeEndpoint(
  url: string,
  body: object | null,
  timeoutMs: number,
  signal?: AbortSignal,
): Promise<ProbeResult> {
  const controller = new AbortController();

  // Wire the external cancellation signal through to our controller so that
  // component unmount (or manual reset) cancels the in-flight fetch immediately.
  let externalAbortHandler: (() => void) | undefined;
  if (signal) {
    if (signal.aborted) {
      return { status: 'fail', message: 'Cancelled' };
    }
    externalAbortHandler = () => controller.abort();
    signal.addEventListener('abort', externalAbortHandler, { once: true });
  }

  // Hard deadline: always fires even if the host never sends a TCP RST.
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  const started = Date.now();

  try {
    const response = await fetch(url, {
      method: body ? 'POST' : 'GET',
      headers: body ? { 'Content-Type': 'application/json' } : undefined,
      body: body ? JSON.stringify(body) : undefined,
      signal: controller.signal,
    });

    const latencyMs = Date.now() - started;

    // Any HTTP answer proves the host is reachable and speaking HTTP; a 404 on
    // a health path still means the endpoint itself is live.
    return response.ok || response.status === 404
      ? { status: 'ok', message: `Reachable (HTTP ${response.status})`, latencyMs }
      : { status: 'fail', message: `Endpoint responded with HTTP ${response.status}`, latencyMs };
  } catch (error) {
    // Distinguish between our own timeout abort and external cancellation.
    if (error instanceof DOMException && error.name === 'AbortError') {
      if (signal?.aborted) {
        return { status: 'fail', message: 'Cancelled' };
      }
      return { status: 'fail', message: `No response within ${timeoutMs}ms` };
    }
    return {
      status: 'fail',
      message: 'Could not reach the endpoint (network error or CORS blocked)',
    };
  } finally {
    clearTimeout(timer);
    if (signal && externalAbortHandler) {
      signal.removeEventListener('abort', externalAbortHandler);
    }
  }
}
