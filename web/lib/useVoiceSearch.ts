/**
 * useVoiceSearch — React hook that wraps the Web Speech API's
 * `SpeechRecognition` interface for the global search overlay.
 *
 * Design goals:
 * - Zero external dependencies; uses only the browser's built-in API.
 * - Degrades gracefully: `supported` is `false` in environments where the API
 *   is unavailable (Firefox without a flag, Node.js during SSR, etc.), and the
 *   UI can hide the mic button accordingly.
 * - Safe cleanup: the recognition session is always aborted when the component
 *   unmounts or the overlay closes, so no orphaned audio streams linger.
 * - Testable: the factory argument lets unit tests inject a mock implementation
 *   without patching `window`.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

// ─── Types ────────────────────────────────────────────────────────────────────

/** The subset of SpeechRecognition we actually use. */
interface SpeechRecognitionLike {
  continuous: boolean;
  interimResults: boolean;
  lang: string;
  onresult: ((event: SpeechRecognitionEventLike) => void) | null;
  onerror: ((event: SpeechRecognitionErrorEventLike) => void) | null;
  onend: (() => void) | null;
  start(): void;
  abort(): void;
}

interface SpeechRecognitionEventLike {
  resultIndex: number;
  results: SpeechRecognitionResultListLike;
}

interface SpeechRecognitionResultListLike {
  length: number;
  [index: number]: SpeechRecognitionResultLike;
}

interface SpeechRecognitionResultLike {
  isFinal: boolean;
  0: { transcript: string };
}

interface SpeechRecognitionErrorEventLike {
  error: string;
}

/** Factory that returns a new SpeechRecognition instance (or null if unavailable). */
export type SpeechRecognitionFactory = () => SpeechRecognitionLike | null;

// ─── Default factory ──────────────────────────────────────────────────────────

/**
 * Resolve the browser's SpeechRecognition constructor, handling the webkit
 * prefix used by Chrome/Edge/Safari. Returns `null` in unsupported environments
 * (Firefox stable, Node.js, jsdom without the API) so callers don't need to
 * guard every access.
 */
export function getDefaultSpeechRecognitionFactory(): SpeechRecognitionFactory {
  return (): SpeechRecognitionLike | null => {
    if (typeof window === 'undefined') return null;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const Ctor = (window as any).SpeechRecognition ?? (window as any).webkitSpeechRecognition;
    if (typeof Ctor !== 'function') return null;
    return new Ctor() as SpeechRecognitionLike;
  };
}

// ─── Hook ─────────────────────────────────────────────────────────────────────

export type VoiceSearchError =
  | 'not-allowed'   // microphone permission denied
  | 'no-speech'     // timeout — user did not speak
  | 'network'       // network required for cloud recognition
  | 'aborted'       // deliberately aborted (not shown to the user)
  | string;         // pass-through for unexpected codes

export interface UseVoiceSearchOptions {
  /**
   * Called whenever the recognised transcript changes (interim or final).
   * The parent component should wire this to its query setter.
   */
  onTranscript: (transcript: string) => void;
  /**
   * BCP-47 language tag for recognition, e.g. `"en-US"`. Defaults to the
   * document language or `"en-US"` when unavailable.
   */
  lang?: string;
  /**
   * Override the SpeechRecognition factory. Primarily for unit tests; the
   * default resolves `window.SpeechRecognition ?? window.webkitSpeechRecognition`.
   */
  recognitionFactory?: SpeechRecognitionFactory;
}

export interface UseVoiceSearchResult {
  /** Whether the browser supports the Web Speech API. */
  supported: boolean;
  /** Whether a recognition session is currently active. */
  listening: boolean;
  /** The most recent error code, or `null` if there has been no error. */
  error: VoiceSearchError | null;
  /** Start a recognition session. No-op when already listening or unsupported. */
  start: () => void;
  /** Abort the current session. No-op when not listening. */
  stop: () => void;
}

export function useVoiceSearch({
  onTranscript,
  lang,
  recognitionFactory,
}: UseVoiceSearchOptions): UseVoiceSearchResult {
  const factory = recognitionFactory ?? getDefaultSpeechRecognitionFactory();

  // Detect support once at hook initialisation time so the value is stable
  // across re-renders and does not trigger repeated factory calls.
  const [supported] = useState<boolean>(() => factory() !== null);
  const [listening, setListening] = useState(false);
  const [error, setError] = useState<VoiceSearchError | null>(null);

  // Keep a ref to the active recognition instance so we can abort it from
  // cleanup effects without capturing stale closure values.
  const recognitionRef = useRef<SpeechRecognitionLike | null>(null);

  // Also keep a stable ref to the latest onTranscript callback so the
  // recognition event handler never closes over a stale version.
  const onTranscriptRef = useRef(onTranscript);
  useEffect(() => {
    onTranscriptRef.current = onTranscript;
  }, [onTranscript]);

  const stop = useCallback(() => {
    if (recognitionRef.current) {
      recognitionRef.current.abort();
      recognitionRef.current = null;
    }
    setListening(false);
  }, []);

  const start = useCallback(() => {
    if (!supported || listening) return;

    const recognition = factory();
    if (!recognition) return;

    setError(null);

    // Use interim results so the query field updates as the user speaks,
    // giving real-time feedback rather than waiting for a final result.
    recognition.continuous = false;
    recognition.interimResults = true;
    recognition.lang =
      lang ??
      (typeof document !== 'undefined' ? document.documentElement.lang : '') ||
      'en-US';

    recognition.onresult = (event: SpeechRecognitionEventLike) => {
      let transcript = '';
      for (let i = event.resultIndex; i < event.results.length; i++) {
        transcript += event.results[i][0].transcript;
      }
      onTranscriptRef.current(transcript);
    };

    recognition.onerror = (event: SpeechRecognitionErrorEventLike) => {
      // 'aborted' is fired when we call abort() ourselves — don't surface it
      // as a user-visible error.
      if (event.error !== 'aborted') {
        setError(event.error as VoiceSearchError);
      }
      setListening(false);
      recognitionRef.current = null;
    };

    recognition.onend = () => {
      // onend always fires after the session ends (including after onerror).
      // Guard against double-clearing the ref.
      recognitionRef.current = null;
      setListening(false);
    };

    recognitionRef.current = recognition;
    setListening(true);
    recognition.start();
  }, [supported, listening, factory, lang]);

  // Abort any in-flight session when the hook unmounts (e.g. the modal closes).
  useEffect(() => {
    return () => {
      if (recognitionRef.current) {
        recognitionRef.current.abort();
        recognitionRef.current = null;
      }
    };
  }, []);

  return { supported, listening, error, start, stop };
}
