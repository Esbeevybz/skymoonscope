/**
 * App-facing entry point for the Soroban metadata decoder.
 *
 * The implementation lives in `public/wasm/sorobanMeta.js` so the Web Worker
 * can `importScripts()` it directly; this re-export keeps normal
 * `from '../lib/sorobanMeta'` imports (and the unit tests) unchanged.
 */
module.exports = require('../public/wasm/sorobanMeta.js');
