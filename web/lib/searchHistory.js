/**
 * Search history — recently visited panels / commands.
 *
 * Pure, DOM-free logic so it can be unit-tested without a browser. Storage I/O
 * is intentionally separated into `loadHistory` / `saveHistory` so callers
 * can provide their own adapter (e.g. a mock in tests).
 */

'use strict';

const STORAGE_KEY = 'sky-moon-scope:search-history';
const MAX_HISTORY = 8;

/**
 * @typedef {Object} HistoryEntry
 * @property {string}  id         - Command id (mirrors SearchCommand.id).
 * @property {string}  title      - Human-readable label shown in the list.
 * @property {string}  [subtitle] - Optional extra detail line.
 * @property {string}  group      - Palette group label.
 * @property {string}  [href]     - Navigation target, if any.
 * @property {string}  [action]   - Non-navigation action id, if any.
 * @property {Object}  [payload]  - Action payload carried over from the command.
 * @property {number}  visitedAt  - Unix timestamp (ms) of the last visit.
 */

/**
 * Return a fresh `HistoryEntry` derived from a `SearchCommand`.
 *
 * @param {Object} command
 * @returns {HistoryEntry}
 */
function commandToEntry(command) {
  return {
    id: command.id,
    title: command.title,
    subtitle: command.subtitle,
    group: command.group,
    href: command.href,
    action: command.action,
    payload: command.payload,
    visitedAt: Date.now(),
  };
}

/**
 * Push `command` to the top of the history list.
 *
 * - Deduplicates by `id` (removes any existing entry for the same command).
 * - Keeps at most `MAX_HISTORY` entries.
 *
 * @param {HistoryEntry[]} history - Current history, newest-first.
 * @param {Object}         command - The command that was just run.
 * @returns {HistoryEntry[]}       - Updated list (new array, newest-first).
 */
function pushHistory(history, command) {
  const list = Array.isArray(history) ? history : [];
  const entry = commandToEntry(command);
  const filtered = list.filter((e) => e.id !== entry.id);
  return [entry, ...filtered].slice(0, MAX_HISTORY);
}

/**
 * Remove a single entry by id.
 *
 * @param {HistoryEntry[]} history
 * @param {string}         id
 * @returns {HistoryEntry[]}
 */
function removeHistoryEntry(history, id) {
  const list = Array.isArray(history) ? history : [];
  return list.filter((e) => e.id !== id);
}

/**
 * Clear the entire history list.
 *
 * @returns {HistoryEntry[]}
 */
function clearHistory() {
  return [];
}

/**
 * Convert history entries to `SearchCommand`-shaped objects so they can be
 * used directly with `filterCommands`.
 *
 * @param {HistoryEntry[]} history
 * @returns {Object[]}
 */
function historyToCommands(history) {
  const list = Array.isArray(history) ? history : [];
  return list.map((entry) => ({
    id: entry.id,
    title: entry.title,
    subtitle: entry.subtitle,
    group: 'Recent',
    href: entry.href,
    action: entry.action,
    payload: entry.payload,
    keywords: ['recent', 'history', 'visited'],
  }));
}

// ── LocalStorage adapter ─────────────────────────────────────────────────────

/**
 * Load history from `localStorage`. Returns `[]` on any error (missing key,
 * corrupt JSON, unavailable storage).
 *
 * @returns {HistoryEntry[]}
 */
function loadHistory() {
  try {
    if (typeof localStorage === 'undefined') return [];
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

/**
 * Persist `history` to `localStorage`. Silently ignores errors (private
 * browsing mode, storage quota, unavailable store).
 *
 * @param {HistoryEntry[]} history
 */
function saveHistory(history) {
  try {
    if (typeof localStorage === 'undefined') return;
    localStorage.setItem(STORAGE_KEY, JSON.stringify(history));
  } catch {
    // ignore
  }
}

module.exports = {
  STORAGE_KEY,
  MAX_HISTORY,
  commandToEntry,
  pushHistory,
  removeHistoryEntry,
  clearHistory,
  historyToCommands,
  loadHistory,
  saveHistory,
};
