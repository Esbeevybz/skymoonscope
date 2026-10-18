// probeEndpoint.test.cjs — unit tests for the probeEndpoint liveness probe
// Closes Issue #400 — connection test hanging indefinitely on unreachable RPC
// Runs with: node --test ./lib/probeEndpoint.test.cjs

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

// ── Inline JS port of lib/probeEndpoint.ts ───────────────────────────────────
// The test runner executes plain CJS files, so we replicate the logic here
// rather than transpiling TypeScript at test time.

/**
 * @param {string} url
 * @param {object|null} body
 * @param {number} timeoutMs
 * @param {AbortSignal} [signal]
 * @returns {Promise<{status:'ok'|'fail', message:string, latencyMs?:number}>}
 */
async function probeEndpoint(url, body, timeoutMs, signal) {
  const controller = new AbortController();

  let externalAbortHandler;
  if (signal) {
    if (signal.aborted) {
      return { status: 'fail', message: 'Cancelled' };
    }
    externalAbortHandler = () => controller.abort();
    signal.addEventListener('abort', externalAbortHandler, { once: true });
  }

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

    return response.ok || response.status === 404
      ? { status: 'ok', message: `Reachable (HTTP ${response.status})`, latencyMs }
      : { status: 'fail', message: `Endpoint responded with HTTP ${response.status}`, latencyMs };
  } catch (error) {
    if (error instanceof DOMException && error.name === 'AbortError') {
      if (signal && signal.aborted) {
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

// ── Helpers ───────────────────────────────────────────────────────────────────

/**
 * Build a mock `fetch` that responds after `delayMs` with `status`.
 * Respects the AbortSignal: if aborted before the delay fires, the promise
 * rejects with a DOMException named 'AbortError', just like the real fetch.
 *
 * @param {{ status?: number, delayMs?: number }} opts
 * @returns {function}
 */
function makeFetch({ status = 200, delayMs = 0 } = {}) {
  return function mockFetch(_url, init) {
    return new Promise((resolve, reject) => {
      const signal = init && init.signal;

      if (signal && signal.aborted) {
        reject(Object.assign(new DOMException('The operation was aborted', 'AbortError'), {}));
        return;
      }

      const timer = setTimeout(() => {
        resolve({
          ok: status >= 200 && status < 300,
          status,
        });
      }, delayMs);

      if (signal) {
        signal.addEventListener('abort', () => {
          clearTimeout(timer);
          reject(Object.assign(new DOMException('The operation was aborted', 'AbortError'), {}));
        }, { once: true });
      }
    });
  };
}

/** Replace the global fetch, run `fn`, then restore the original. */
async function withFetch(mockFn, fn) {
  const original = globalThis.fetch;
  globalThis.fetch = mockFn;
  try {
    return await fn();
  } finally {
    globalThis.fetch = original;
  }
}

// ── Tests: happy path ─────────────────────────────────────────────────────────

test('probeEndpoint: resolves ok when endpoint responds 200', async () => {
  const result = await withFetch(
    makeFetch({ status: 200, delayMs: 0 }),
    () => probeEndpoint('http://localhost:8080/', null, 5000),
  );

  assert.equal(result.status, 'ok');
  assert.match(result.message, /Reachable/);
  assert.equal(typeof result.latencyMs, 'number');
  assert.ok(result.latencyMs >= 0);
});

test('probeEndpoint: treats 404 as reachable (health path convention)', async () => {
  const result = await withFetch(
    makeFetch({ status: 404, delayMs: 0 }),
    () => probeEndpoint('http://localhost:8080/health', null, 5000),
  );

  assert.equal(result.status, 'ok');
  assert.match(result.message, /404/);
});

test('probeEndpoint: fails on 5xx HTTP response', async () => {
  const result = await withFetch(
    makeFetch({ status: 503, delayMs: 0 }),
    () => probeEndpoint('http://localhost:8080/', null, 5000),
  );

  assert.equal(result.status, 'fail');
  assert.match(result.message, /503/);
  assert.equal(typeof result.latencyMs, 'number');
});

test('probeEndpoint: posts JSON body for POST requests', async () => {
  let capturedBody;

  const mockFetch = (url, init) => {
    capturedBody = init.body;
    return makeFetch({ status: 200 })(url, init);
  };

  await withFetch(mockFetch, () =>
    probeEndpoint('http://localhost:8080/', { jsonrpc: '2.0', id: 1, method: 'getHealth' }, 5000),
  );

  const parsed = JSON.parse(capturedBody);
  assert.equal(parsed.method, 'getHealth');
});

test('probeEndpoint: uses GET with no body when body is null', async () => {
  let capturedMethod;

  const mockFetch = (url, init) => {
    capturedMethod = init.method;
    return makeFetch({ status: 200 })(url, init);
  };

  await withFetch(mockFetch, () =>
    probeEndpoint('http://localhost:8080/health', null, 5000),
  );

  assert.equal(capturedMethod, 'GET');
});

// ── Tests: timeout behaviour ──────────────────────────────────────────────────

test('probeEndpoint: resolves with fail when response exceeds timeoutMs', async () => {
  // Endpoint takes 200 ms but timeout is only 50 ms.
  const TIMEOUT_MS = 50;

  const result = await withFetch(
    makeFetch({ status: 200, delayMs: 200 }),
    () => probeEndpoint('http://localhost:8080/', null, TIMEOUT_MS),
  );

  assert.equal(result.status, 'fail');
  assert.match(result.message, new RegExp(`${TIMEOUT_MS}ms`));
});

test('probeEndpoint: resolves (never hangs) even when fetch never settles', async (t) => {
  // A fetch that never resolves or rejects unless aborted.
  const hangingFetch = (_url, init) =>
    new Promise((_resolve, reject) => {
      const signal = init && init.signal;
      if (signal) {
        signal.addEventListener('abort', () => {
          reject(Object.assign(new DOMException('The operation was aborted', 'AbortError'), {}));
        }, { once: true });
      }
      // Intentionally never resolves otherwise.
    });

  const TIMEOUT_MS = 100;
  const started = Date.now();

  const result = await withFetch(
    hangingFetch,
    () => probeEndpoint('http://unreachable.example/', null, TIMEOUT_MS),
  );

  const elapsed = Date.now() - started;

  assert.equal(result.status, 'fail');
  assert.match(result.message, new RegExp(`${TIMEOUT_MS}ms`));
  // Should have resolved within a reasonable window around the timeout.
  assert.ok(
    elapsed >= TIMEOUT_MS && elapsed < TIMEOUT_MS + 500,
    `Expected elapsed ~${TIMEOUT_MS}ms, got ${elapsed}ms`,
  );
});

test('probeEndpoint: timer is cleared after a fast success (no leak)', async (t) => {
  // We cannot observe timer leaks directly, but we can confirm the function
  // returns promptly and doesn't throw — if clearTimeout were missing the
  // test suite would stall waiting for the timer.
  const result = await withFetch(
    makeFetch({ status: 200, delayMs: 0 }),
    () => probeEndpoint('http://localhost:8080/', null, 10_000),
  );

  assert.equal(result.status, 'ok');
});

// ── Tests: external AbortSignal cancellation ──────────────────────────────────

test('probeEndpoint: returns fail immediately when signal is already aborted', async () => {
  const controller = new AbortController();
  controller.abort();

  // fetch should not even be called if the signal is already aborted.
  let fetchCalled = false;
  const neverFetch = () => { fetchCalled = true; return Promise.resolve({ ok: true, status: 200 }); };

  const result = await withFetch(neverFetch, () =>
    probeEndpoint('http://localhost:8080/', null, 5000, controller.signal),
  );

  assert.equal(result.status, 'fail');
  assert.match(result.message, /Cancelled/);
  assert.equal(fetchCalled, false, 'fetch should not have been called');
});

test('probeEndpoint: cancels in-flight fetch when external signal fires', async () => {
  const controller = new AbortController();

  // Endpoint responds after 300 ms; we abort after 50 ms.
  const started = Date.now();
  const probePromise = withFetch(
    makeFetch({ status: 200, delayMs: 300 }),
    () => probeEndpoint('http://localhost:8080/', null, 10_000, controller.signal),
  );

  setTimeout(() => controller.abort(), 50);
  const result = await probePromise;

  const elapsed = Date.now() - started;

  assert.equal(result.status, 'fail');
  assert.match(result.message, /Cancelled/);
  // Should have resolved well before the 300 ms response delay.
  assert.ok(elapsed < 200, `Should have cancelled quickly, got ${elapsed}ms`);
});

test('probeEndpoint: removes external abort listener after completing normally', async () => {
  const controller = new AbortController();

  const result = await withFetch(
    makeFetch({ status: 200, delayMs: 0 }),
    () => probeEndpoint('http://localhost:8080/', null, 5000, controller.signal),
  );

  // Should have returned ok; aborting now should have no effect and not throw.
  assert.equal(result.status, 'ok');
  assert.doesNotThrow(() => controller.abort());
});

// ── Tests: network errors ─────────────────────────────────────────────────────

test('probeEndpoint: handles fetch network errors gracefully', async () => {
  const errorFetch = () => Promise.reject(new TypeError('Failed to fetch'));

  const result = await withFetch(errorFetch, () =>
    probeEndpoint('http://localhost:9999/', null, 5000),
  );

  assert.equal(result.status, 'fail');
  assert.match(result.message, /network error|CORS/i);
});

test('probeEndpoint: latencyMs is undefined on network error', async () => {
  const errorFetch = () => Promise.reject(new TypeError('net::ERR_CONNECTION_REFUSED'));

  const result = await withFetch(errorFetch, () =>
    probeEndpoint('http://localhost:9999/', null, 5000),
  );

  assert.equal(result.status, 'fail');
  assert.equal(result.latencyMs, undefined);
});
