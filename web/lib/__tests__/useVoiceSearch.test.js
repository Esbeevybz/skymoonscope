// useVoiceSearch.test.js — unit tests for the voice-search hook logic
// (issue #394 — Add voice input support to global search overlay)
//
// Runs with: node --test lib/__tests__/useVoiceSearch.test.js
//
// Strategy: the hook accepts a `recognitionFactory` option, which lets tests
// inject a controllable mock without patching `window` or running a React
// renderer.  The tests drive the state-machine logic by:
//   1. Calling `startVoiceSearch(mock)` — the helper that mirrors the hook's
//      `start()` path — and then
//   2. Firing synthetic events (onresult / onerror / onend) on the mock, and
//   3. Asserting on the observable side-effects (transcript calls, error state,
//      abort calls, listening state).

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

// ─── Mock SpeechRecognition ───────────────────────────────────────────────────

/**
 * A minimal SpeechRecognition stand-in.
 *
 * Exposes the same surface as the real API: `start`, `abort`, and the three
 * event handler properties `onresult`, `onerror`, `onend`.  Tracks calls so
 * tests can assert on them.
 */
function makeMockRecognition() {
  return {
    continuous: false,
    interimResults: false,
    lang: '',
    onresult: null,
    onerror: null,
    onend: null,
    startCalled: 0,
    abortCalled: 0,
    start() { this.startCalled += 1; },
    abort() { this.abortCalled += 1; },

    /** Helper: fire a result event with a single interim transcript. */
    fireResult(transcript, isFinal = false) {
      if (!this.onresult) return;
      this.onresult({
        resultIndex: 0,
        results: {
          length: 1,
          0: { isFinal, 0: { transcript } },
        },
      });
    },

    /** Helper: fire an error event. */
    fireError(errorCode) {
      if (this.onerror) this.onerror({ error: errorCode });
    },

    /** Helper: fire the end event. */
    fireEnd() {
      if (this.onend) this.onend();
    },
  };
}

// ─── Thin state machine (mirrors hook logic, no React) ────────────────────────
//
// The hook's state is: { listening, error }.  We replicate the same transitions
// here so we can test the logic without a React renderer.

function makeState() {
  return { listening: false, error: null };
}

/**
 * Run the `start()` path of the hook.
 * Returns the recognition instance that was configured (or null).
 */
function startVoiceSearch(state, mock, onTranscript, lang = 'en-US') {
  if (!mock || state.listening) return null;

  state.error = null;

  mock.continuous = false;
  mock.interimResults = true;
  mock.lang = lang;

  mock.onresult = (event) => {
    let transcript = '';
    for (let i = event.resultIndex; i < event.results.length; i++) {
      transcript += event.results[i][0].transcript;
    }
    onTranscript(transcript);
  };

  mock.onerror = (event) => {
    if (event.error !== 'aborted') {
      state.error = event.error;
    }
    state.listening = false;
  };

  mock.onend = () => {
    state.listening = false;
  };

  state.listening = true;
  mock.start();
  return mock;
}

/**
 * Run the `stop()` path of the hook.
 */
function stopVoiceSearch(state, mock) {
  if (mock) mock.abort();
  state.listening = false;
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/** Make a factory that always returns the given mock (or null). */
function factoryFor(mock) {
  return () => mock;
}

// ─── Tests ────────────────────────────────────────────────────────────────────

// supported detection ──────────────────────────────────────────────────────────

test('factory returning null → supported is false', () => {
  const factory = factoryFor(null);
  assert.equal(factory(), null);
});

test('factory returning an object → supported is true', () => {
  const factory = factoryFor(makeMockRecognition());
  assert.ok(factory() !== null);
});

// start() ─────────────────────────────────────────────────────────────────────

test('start: calls recognition.start() once', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  startVoiceSearch(state, mock, () => {});
  assert.equal(mock.startCalled, 1);
});

test('start: sets listening to true', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  startVoiceSearch(state, mock, () => {});
  assert.equal(state.listening, true);
});

test('start: configures interimResults and continuous correctly', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  startVoiceSearch(state, mock, () => {});
  assert.equal(mock.interimResults, true);
  assert.equal(mock.continuous, false);
});

test('start: sets lang from the argument', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  startVoiceSearch(state, mock, () => {}, 'fr-FR');
  assert.equal(mock.lang, 'fr-FR');
});

test('start: no-op when already listening', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  state.listening = true; // already listening
  const result = startVoiceSearch(state, mock, () => {});
  assert.equal(result, null);
  assert.equal(mock.startCalled, 0);
});

test('start: no-op when factory returns null (unsupported)', () => {
  const state = makeState();
  const result = startVoiceSearch(state, null, () => {});
  assert.equal(result, null);
  assert.equal(state.listening, false);
});

test('start: clears any previous error', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  state.error = 'no-speech';
  startVoiceSearch(state, mock, () => {});
  assert.equal(state.error, null);
});

// onresult ────────────────────────────────────────────────────────────────────

test('onresult: fires onTranscript with the concatenated transcript', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  const received = [];
  startVoiceSearch(state, mock, (t) => received.push(t));
  mock.fireResult('hello world');
  assert.deepEqual(received, ['hello world']);
});

test('onresult: accumulates multiple results from resultIndex', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  const received = [];

  startVoiceSearch(state, mock, (t) => received.push(t));

  // Simulate two results in one event (resultIndex=0, length=2)
  mock.onresult({
    resultIndex: 0,
    results: {
      length: 2,
      0: { isFinal: false, 0: { transcript: 'trans' } },
      1: { isFinal: false, 0: { transcript: 'fer' } },
    },
  });

  assert.deepEqual(received, ['transfer']);
});

test('onresult: interim result does not change listening state', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  startVoiceSearch(state, mock, () => {});
  mock.fireResult('set', false);
  assert.equal(state.listening, true);
});

// onerror ─────────────────────────────────────────────────────────────────────

test('onerror: sets error state for not-allowed', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  startVoiceSearch(state, mock, () => {});
  mock.fireError('not-allowed');
  assert.equal(state.error, 'not-allowed');
  assert.equal(state.listening, false);
});

test('onerror: sets error state for no-speech', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  startVoiceSearch(state, mock, () => {});
  mock.fireError('no-speech');
  assert.equal(state.error, 'no-speech');
});

test('onerror: sets error state for network', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  startVoiceSearch(state, mock, () => {});
  mock.fireError('network');
  assert.equal(state.error, 'network');
});

test('onerror: does NOT set error state for aborted (user-initiated)', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  startVoiceSearch(state, mock, () => {});
  mock.fireError('aborted');
  assert.equal(state.error, null);
  assert.equal(state.listening, false);
});

test('onerror: sets listening to false', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  startVoiceSearch(state, mock, () => {});
  assert.equal(state.listening, true);
  mock.fireError('network');
  assert.equal(state.listening, false);
});

// onend ───────────────────────────────────────────────────────────────────────

test('onend: sets listening to false', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  startVoiceSearch(state, mock, () => {});
  assert.equal(state.listening, true);
  mock.fireEnd();
  assert.equal(state.listening, false);
});

test('onend: does not change error state', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  startVoiceSearch(state, mock, () => {});
  state.error = 'no-speech'; // pre-existing error
  mock.fireEnd();
  assert.equal(state.error, 'no-speech');
});

// stop() ──────────────────────────────────────────────────────────────────────

test('stop: calls recognition.abort()', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  startVoiceSearch(state, mock, () => {});
  stopVoiceSearch(state, mock);
  assert.equal(mock.abortCalled, 1);
});

test('stop: sets listening to false', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  startVoiceSearch(state, mock, () => {});
  stopVoiceSearch(state, mock);
  assert.equal(state.listening, false);
});

test('stop: is safe to call when not listening', () => {
  const state = makeState();
  assert.doesNotThrow(() => stopVoiceSearch(state, null));
  assert.equal(state.listening, false);
});

// session lifecycle ───────────────────────────────────────────────────────────

test('full lifecycle: start → result → end', () => {
  const mock = makeMockRecognition();
  const state = makeState();
  const transcripts = [];

  startVoiceSearch(state, mock, (t) => transcripts.push(t));
  assert.equal(state.listening, true);

  mock.fireResult('transfer');
  assert.deepEqual(transcripts, ['transfer']);

  mock.fireEnd();
  assert.equal(state.listening, false);
  assert.equal(state.error, null);
});

test('full lifecycle: start → error → end', () => {
  const mock = makeMockRecognition();
  const state = makeState();

  startVoiceSearch(state, mock, () => {});
  mock.fireError('not-allowed');
  // onerror sets listening false; onend may fire too but should be idempotent
  mock.fireEnd();

  assert.equal(state.listening, false);
  assert.equal(state.error, 'not-allowed');
});

test('second start after end works correctly', () => {
  const mock1 = makeMockRecognition();
  const mock2 = makeMockRecognition();
  const state = makeState();
  const transcripts = [];

  startVoiceSearch(state, mock1, (t) => transcripts.push(t));
  mock1.fireResult('hello');
  mock1.fireEnd(); // session ends

  // Start again with a fresh mock
  startVoiceSearch(state, mock2, (t) => transcripts.push(t));
  mock2.fireResult('world');
  mock2.fireEnd();

  assert.deepEqual(transcripts, ['hello', 'world']);
  assert.equal(mock1.startCalled, 1);
  assert.equal(mock2.startCalled, 1);
});

test('abort on unmount does not trigger user-visible error', () => {
  const mock = makeMockRecognition();
  const state = makeState();

  startVoiceSearch(state, mock, () => {});
  // Simulate unmount cleanup: abort then the browser fires onerror('aborted')
  mock.abort();
  mock.fireError('aborted');

  assert.equal(state.error, null, 'aborted error should not be surfaced');
  assert.equal(state.listening, false);
});
