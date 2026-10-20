/**
 * Dedicated Web Worker for WASM decoding and validation.
 *
 * Parsing a multi-megabyte contract on the main thread blocks paint and input
 * handling, which showed up as typing lag while an upload was in flight. The
 * whole decode now happens here and only the (small) JSON report crosses back
 * over the message channel.
 *
 * This is a classic worker served straight from `public/`, deliberately not a
 * bundler worker entry: `importScripts` resolves at runtime, so the same file
 * works identically under webpack, Turbopack and `next dev`.
 *
 * Protocol
 *   in : { type: 'ping' }
 *   out: { type: 'pong' }
 *   in : { id: string, type: 'validate', buffer: ArrayBuffer, maxBytes?: number }
 *   out: { id: string, type: 'result', report: WasmValidationReport }
 *        { id: string, type: 'error',  message: string }
 *
 * The caller only sends real work after `pong` comes back, so a worker that
 * cannot boot degrades to main-thread validation instead of failing uploads.
 *
 * Fix #409: the original code placed the SkyMoonScopeWasmValidation destructure
 * at the top level, outside onmessage. If importScripts threw or failed (e.g.
 * 404, CSP, network hiccup, or the Node VM sandbox used in tests), the
 * TypeError from destructuring undefined would kill the worker script before
 * onmessage was ever registered — so the ping handshake never responded and
 * every upload silently fell back to main-thread parsing but with no error
 * surfaced. The destructure now lives inside onmessage, after the ping fast-
 * path, so a bad importScripts only affects validate calls, and only those
 * calls receive a proper error reply rather than the worker dying entirely.
 */

try {
  importScripts('/wasm/sorobanMeta.js');
} catch {
  // Ignore if unavailable
}
importScripts('/wasm/wasmValidation.js');

self.onmessage = (event) => {
  const data = event && event.data;
  if (!data) return;

  if (data.type === 'ping') {
    self.postMessage({ type: 'pong' });
    return;
  }

  if (data.type !== 'validate') {
    return;
  }

  const { id, buffer, maxBytes } = data;

  // Resolve validateWasmModule here, inside onmessage, so that a failure in
  // the top-level importScripts call above does not prevent onmessage from
  // being registered. If the import succeeded, this is a fast property lookup;
  // if it failed, we post an informative error instead of killing the worker.
  const api = self.SkyMoonScopeWasmValidation;
  if (!api || typeof api.validateWasmModule !== 'function') {
    self.postMessage({
      id,
      type: 'error',
      message: 'WASM validation module failed to load (importScripts did not populate SkyMoonScopeWasmValidation)',
    });
    return;
  }

  try {
    const report = api.validateWasmModule(buffer, { maxBytes });
    self.postMessage({ id, type: 'result', report });
  } catch (error) {
    self.postMessage({
      id,
      type: 'error',
      message: error instanceof Error ? error.message : 'WASM validation failed',
    });
  }
};
