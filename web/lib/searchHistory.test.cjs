// searchHistory.test.cjs — unit tests for search history feature
// Runs with: node --test ./lib/searchHistory.test.cjs

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const {
  MAX_HISTORY,
  commandToEntry,
  pushHistory,
  removeHistoryEntry,
  clearHistory,
  historyToCommands,
} = require('./searchHistory');

// ── commandToEntry ──────────────────────────────────────────────────────────

test('commandToEntry: maps a navigation command to an entry', () => {
  const cmd = {
    id: 'nav-settings',
    title: 'Settings',
    subtitle: 'Configure RPC endpoints',
    group: 'Preferences',
    href: '/settings',
    keywords: ['rpc'],
  };
  const entry = commandToEntry(cmd);
  assert.equal(entry.id, 'nav-settings');
  assert.equal(entry.title, 'Settings');
  assert.equal(entry.subtitle, 'Configure RPC endpoints');
  assert.equal(entry.group, 'Preferences');
  assert.equal(entry.href, '/settings');
  assert.equal(entry.action, undefined);
  assert.ok(typeof entry.visitedAt === 'number', 'visitedAt should be a number');
  assert.ok(entry.visitedAt > 0, 'visitedAt should be a positive timestamp');
});

test('commandToEntry: maps an action command to an entry', () => {
  const cmd = {
    id: 'fn-transfer',
    title: 'transfer(from: address, to: address, amount: u128)',
    group: 'Contract functions',
    action: 'select-function',
    payload: { name: 'transfer' },
  };
  const entry = commandToEntry(cmd);
  assert.equal(entry.id, 'fn-transfer');
  assert.equal(entry.action, 'select-function');
  assert.deepEqual(entry.payload, { name: 'transfer' });
  assert.equal(entry.href, undefined);
});

test('commandToEntry: sets visitedAt close to Date.now()', () => {
  const before = Date.now();
  const entry = commandToEntry({ id: 'x', title: 'X', group: 'G' });
  const after = Date.now();
  assert.ok(entry.visitedAt >= before, 'visitedAt should be >= before timestamp');
  assert.ok(entry.visitedAt <= after, 'visitedAt should be <= after timestamp');
});

// ── pushHistory ─────────────────────────────────────────────────────────────

test('pushHistory: adds the first item to an empty history', () => {
  const cmd = { id: 'nav-explorer', title: 'Explorer', group: 'Navigation', href: '/?tab=explorer' };
  const result = pushHistory([], cmd);
  assert.equal(result.length, 1);
  assert.equal(result[0].id, 'nav-explorer');
});

test('pushHistory: prepends the new item (newest first)', () => {
  const cmd1 = { id: 'a', title: 'A', group: 'G', href: '/a' };
  const cmd2 = { id: 'b', title: 'B', group: 'G', href: '/b' };
  let history = pushHistory([], cmd1);
  history = pushHistory(history, cmd2);
  assert.equal(history[0].id, 'b');
  assert.equal(history[1].id, 'a');
});

test('pushHistory: deduplicates — moves existing entry to top', () => {
  const cmd1 = { id: 'a', title: 'A', group: 'G', href: '/a' };
  const cmd2 = { id: 'b', title: 'B', group: 'G', href: '/b' };
  let history = pushHistory([], cmd1);
  history = pushHistory(history, cmd2);
  history = pushHistory(history, cmd1); // revisit 'a'
  assert.equal(history.length, 2, 'no duplicates');
  assert.equal(history[0].id, 'a', 'revisited item is at the top');
  assert.equal(history[1].id, 'b');
});

test('pushHistory: respects MAX_HISTORY limit', () => {
  let history = [];
  for (let i = 0; i < MAX_HISTORY + 5; i++) {
    history = pushHistory(history, { id: `cmd-${i}`, title: `Cmd ${i}`, group: 'G', href: `/c${i}` });
  }
  assert.equal(history.length, MAX_HISTORY, `should keep at most ${MAX_HISTORY} entries`);
});

test('pushHistory: oldest entries are dropped when limit is reached', () => {
  let history = [];
  for (let i = 0; i < MAX_HISTORY; i++) {
    history = pushHistory(history, { id: `cmd-${i}`, title: `Cmd ${i}`, group: 'G', href: `/c${i}` });
  }
  // history[MAX_HISTORY - 1] is the oldest (cmd-0 was pushed first)
  const oldest = history[history.length - 1];
  // Push one more — oldest should be dropped
  history = pushHistory(history, { id: 'cmd-new', title: 'New', group: 'G', href: '/new' });
  assert.equal(history.length, MAX_HISTORY);
  assert.equal(history[0].id, 'cmd-new');
  assert.ok(
    !history.some((e) => e.id === oldest.id),
    'oldest entry should have been dropped',
  );
});

test('pushHistory: tolerates a non-array history argument', () => {
  const cmd = { id: 'x', title: 'X', group: 'G', href: '/x' };
  const result = pushHistory(null, cmd);
  assert.equal(result.length, 1);
  assert.equal(result[0].id, 'x');
});

// ── removeHistoryEntry ──────────────────────────────────────────────────────

test('removeHistoryEntry: removes the matching entry', () => {
  const cmd1 = { id: 'a', title: 'A', group: 'G', href: '/a' };
  const cmd2 = { id: 'b', title: 'B', group: 'G', href: '/b' };
  let history = pushHistory(pushHistory([], cmd1), cmd2);
  history = removeHistoryEntry(history, 'a');
  assert.equal(history.length, 1);
  assert.equal(history[0].id, 'b');
});

test('removeHistoryEntry: is a no-op for a missing id', () => {
  const cmd = { id: 'a', title: 'A', group: 'G', href: '/a' };
  const history = pushHistory([], cmd);
  const result = removeHistoryEntry(history, 'does-not-exist');
  assert.equal(result.length, 1);
});

test('removeHistoryEntry: tolerates an empty history', () => {
  const result = removeHistoryEntry([], 'any');
  assert.deepEqual(result, []);
});

// ── clearHistory ─────────────────────────────────────────────────────────────

test('clearHistory: returns an empty array', () => {
  assert.deepEqual(clearHistory(), []);
});

// ── historyToCommands ─────────────────────────────────────────────────────────

test('historyToCommands: converts entries to SearchCommand-shaped objects', () => {
  const cmd = { id: 'nav-settings', title: 'Settings', group: 'Preferences', href: '/settings' };
  const history = pushHistory([], cmd);
  const commands = historyToCommands(history);
  assert.equal(commands.length, 1);
  const c = commands[0];
  assert.equal(c.id, 'nav-settings');
  assert.equal(c.title, 'Settings');
  assert.equal(c.group, 'Recent');
  assert.ok(Array.isArray(c.keywords));
  assert.ok(c.keywords.includes('recent'));
  assert.ok(c.keywords.includes('history'));
  assert.ok(c.keywords.includes('visited'));
});

test('historyToCommands: preserves href and action from entries', () => {
  const cmd1 = { id: 'nav-explorer', title: 'Explorer', group: 'Navigation', href: '/?tab=explorer' };
  const cmd2 = { id: 'fn-transfer', title: 'transfer()', group: 'Contract functions', action: 'select-function', payload: { name: 'transfer' } };
  let history = pushHistory([], cmd1);
  history = pushHistory(history, cmd2);
  const commands = historyToCommands(history);
  const explorerCmd = commands.find((c) => c.id === 'nav-explorer');
  const fnCmd = commands.find((c) => c.id === 'fn-transfer');
  assert.equal(explorerCmd.href, '/?tab=explorer');
  assert.equal(fnCmd.action, 'select-function');
  assert.deepEqual(fnCmd.payload, { name: 'transfer' });
});

test('historyToCommands: returns empty array for empty history', () => {
  assert.deepEqual(historyToCommands([]), []);
  assert.deepEqual(historyToCommands(null), []);
});

test('historyToCommands: preserves newest-first order', () => {
  const cmd1 = { id: 'a', title: 'A', group: 'G', href: '/a' };
  const cmd2 = { id: 'b', title: 'B', group: 'G', href: '/b' };
  let history = pushHistory([], cmd1);
  history = pushHistory(history, cmd2);
  const commands = historyToCommands(history);
  assert.equal(commands[0].id, 'b', 'newest first');
  assert.equal(commands[1].id, 'a');
});

// ── Integration: full workflow ───────────────────────────────────────────────

test('search history full workflow: visit → deduplicate → remove → clear', () => {
  const cmds = [
    { id: 'nav-explorer', title: 'Explorer', group: 'Navigation', href: '/?tab=explorer' },
    { id: 'nav-settings', title: 'Settings', group: 'Preferences', href: '/settings' },
    { id: 'fn-transfer', title: 'transfer()', group: 'Contract functions', action: 'select-function', payload: { name: 'transfer' } },
  ];

  let history = [];

  // Visit all three
  for (const cmd of cmds) {
    history = pushHistory(history, cmd);
  }
  assert.equal(history.length, 3);
  assert.equal(history[0].id, 'fn-transfer', 'last visited is first');

  // Re-visit Explorer — should bubble to top, no duplicate
  history = pushHistory(history, cmds[0]);
  assert.equal(history.length, 3);
  assert.equal(history[0].id, 'nav-explorer');

  // Remove Settings
  history = removeHistoryEntry(history, 'nav-settings');
  assert.equal(history.length, 2);
  assert.ok(!history.some((e) => e.id === 'nav-settings'));

  // Clear all
  history = clearHistory();
  assert.deepEqual(history, []);
});
