// keyboard-nav.test.cjs
// Tests for:
//   1. Heatmap color scale / state reset when a new contract is loaded
//   2. FunctionSidebar keyboard navigation (ArrowUp/Down, Home, End, Enter/Space)
//   3. NetworkSwitcher keyboard navigation (ArrowUp/Down, Home, End, Escape)
//   4. ResourceHeatmap tab keyboard navigation (ArrowLeft/Right, Home, End)
//
// Runs with: node --test ./components/keyboard-nav.test.cjs

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

// ── Helpers ──────────────────────────────────────────────────────────────────

/**
 * Minimal simulation of ResourceHeatmap state management.
 * Mirrors the useEffect reset and tab keyboard-nav logic.
 */
function createHeatmapState(initialCost) {
  let activeTab = 'gauges';
  let hoveredCellId = null;
  let hoveredRamId = null;
  let hoveredSegmentId = null;
  let hoveredKey = null;
  let cost = { ...initialCost };

  const TABS = ['gauges', 'ram', 'hotspot', 'footprint'];

  function resetIfNewContract(newCost) {
    const changed =
      newCost.cpu_instructions        !== cost.cpu_instructions ||
      newCost.ram_bytes               !== cost.ram_bytes ||
      newCost.ledger_read_bytes       !== cost.ledger_read_bytes ||
      newCost.ledger_write_bytes      !== cost.ledger_write_bytes ||
      newCost.transaction_size_bytes  !== cost.transaction_size_bytes;

    if (changed) {
      activeTab = 'gauges';
      hoveredCellId = null;
      hoveredRamId = null;
      hoveredSegmentId = null;
      hoveredKey = null;
      cost = { ...newCost };
    }
  }

  function setActiveTab(tab) {
    if (TABS.includes(tab)) activeTab = tab;
  }

  function tabKeyDown(key) {
    const idx = TABS.indexOf(activeTab);
    if (key === 'ArrowRight') {
      const next = (idx + 1) % TABS.length;
      activeTab = TABS[next];
      return next;
    }
    if (key === 'ArrowLeft') {
      const prev = (idx - 1 + TABS.length) % TABS.length;
      activeTab = TABS[prev];
      return prev;
    }
    if (key === 'Home') { activeTab = TABS[0]; return 0; }
    if (key === 'End')  { activeTab = TABS[TABS.length - 1]; return TABS.length - 1; }
    return idx;
  }

  return {
    get activeTab()       { return activeTab; },
    get hoveredCellId()   { return hoveredCellId; },
    get hoveredRamId()    { return hoveredRamId; },
    get hoveredSegmentId(){ return hoveredSegmentId; },
    get hoveredKey()      { return hoveredKey; },
    setActiveTab,
    setHoveredCellId(id) { hoveredCellId = id; },
    setHoveredRamId(id)  { hoveredRamId = id; },
    setHoveredSegmentId(id){ hoveredSegmentId = id; },
    setHoveredKey(k)     { hoveredKey = k; },
    resetIfNewContract,
    tabKeyDown,
    TABS,
  };
}

/**
 * Minimal simulation of FunctionSidebar keyboard navigation.
 */
function createFunctionSidebarState(functions, initialSelectedName) {
  let selected = functions.find(f => f.name === initialSelectedName) ?? functions[0];

  function handleKeyDown(key, currentIdx) {
    const len = functions.length;
    if (len === 0) return currentIdx;
    if (key === 'ArrowDown') {
      const next = (currentIdx + 1) % len;
      selected = functions[next];
      return next;
    }
    if (key === 'ArrowUp') {
      const prev = (currentIdx - 1 + len) % len;
      selected = functions[prev];
      return prev;
    }
    if (key === 'Home') { selected = functions[0]; return 0; }
    if (key === 'End')  { selected = functions[len - 1]; return len - 1; }
    if (key === 'Enter' || key === ' ') { /* selection already set */ return currentIdx; }
    return currentIdx;
  }

  return {
    get selected() { return selected; },
    handleKeyDown,
  };
}

/**
 * Minimal simulation of NetworkSwitcher keyboard navigation.
 */
function createNetworkSwitcherState(networks, initialId) {
  let activeId = initialId;
  let isOpen = false;
  let focusedIdx = -1;

  function open() {
    isOpen = true;
    // Auto-focus the currently selected network
    focusedIdx = networks.findIndex(n => n.id === activeId);
    if (focusedIdx < 0) focusedIdx = 0;
  }

  function handleKeyDown(key) {
    if (!isOpen) return;
    if (key === 'Escape') { isOpen = false; focusedIdx = -1; return; }
    if (key === 'ArrowDown') {
      focusedIdx = (focusedIdx + 1) % networks.length;
      return;
    }
    if (key === 'ArrowUp') {
      focusedIdx = (focusedIdx - 1 + networks.length) % networks.length;
      return;
    }
    if (key === 'Home') { focusedIdx = 0; return; }
    if (key === 'End')  { focusedIdx = networks.length - 1; return; }
    if (key === 'Enter' || key === ' ') {
      if (focusedIdx >= 0 && focusedIdx < networks.length) {
        activeId = networks[focusedIdx].id;
        isOpen = false;
        focusedIdx = -1;
      }
    }
  }

  return {
    get activeId()    { return activeId; },
    get isOpen()      { return isOpen; },
    get focusedIdx()  { return focusedIdx; },
    open,
    handleKeyDown,
  };
}

// ── Test data ─────────────────────────────────────────────────────────────────

const BASE_COST = {
  cpu_instructions: 1_000_000,
  ram_bytes: 512_000,
  ledger_read_bytes: 10_000,
  ledger_write_bytes: 5_000,
  transaction_size_bytes: 2_000,
};

const FUNCTIONS = [
  { name: 'transfer', inputs: [{ name: 'amount', type: 'i128' }] },
  { name: 'approve',  inputs: [{ name: 'spender', type: 'address' }, { name: 'amount', type: 'i128' }] },
  { name: 'balance',  inputs: [] },
  { name: 'mint',     inputs: [{ name: 'to', type: 'address' }, { name: 'amount', type: 'i128' }] },
];

const NETWORKS = [
  { id: 'mainnet',   name: 'Mainnet (Public)' },
  { id: 'testnet',   name: 'Testnet' },
  { id: 'futurenet', name: 'Futurenet' },
  { id: 'localhost', name: 'Localhost' },
];

// ── ResourceHeatmap – color scale / state reset ───────────────────────────────

test('Heatmap: initial state defaults to gauges tab with no hovered items', () => {
  const hm = createHeatmapState(BASE_COST);
  assert.equal(hm.activeTab, 'gauges');
  assert.equal(hm.hoveredCellId, null);
  assert.equal(hm.hoveredRamId, null);
  assert.equal(hm.hoveredSegmentId, null);
  assert.equal(hm.hoveredKey, null);
});

test('Heatmap: hover state and active tab persist within same contract load', () => {
  const hm = createHeatmapState(BASE_COST);
  hm.setActiveTab('hotspot');
  hm.setHoveredCellId('def-2');
  hm.setHoveredRamId('ram-1');
  hm.setHoveredSegmentId('r0');
  hm.setHoveredKey('contract_instance');

  // Same cost — nothing should reset
  hm.resetIfNewContract(BASE_COST);

  assert.equal(hm.activeTab, 'hotspot');
  assert.equal(hm.hoveredCellId, 'def-2');
  assert.equal(hm.hoveredRamId, 'ram-1');
  assert.equal(hm.hoveredSegmentId, 'r0');
  assert.equal(hm.hoveredKey, 'contract_instance');
});

test('Heatmap: all hover state and activeTab reset when cpu_instructions changes', () => {
  const hm = createHeatmapState(BASE_COST);
  hm.setActiveTab('ram');
  hm.setHoveredCellId('def-0');
  hm.setHoveredRamId('ram-3');

  const newCost = { ...BASE_COST, cpu_instructions: 50_000_000 };
  hm.resetIfNewContract(newCost);

  assert.equal(hm.activeTab, 'gauges', 'activeTab must reset to gauges');
  assert.equal(hm.hoveredCellId, null);
  assert.equal(hm.hoveredRamId, null);
  assert.equal(hm.hoveredSegmentId, null);
  assert.equal(hm.hoveredKey, null);
});

test('Heatmap: state resets when ram_bytes changes', () => {
  const hm = createHeatmapState(BASE_COST);
  hm.setActiveTab('footprint');
  hm.setHoveredSegmentId('w1');

  hm.resetIfNewContract({ ...BASE_COST, ram_bytes: 8_000_000 });

  assert.equal(hm.activeTab, 'gauges');
  assert.equal(hm.hoveredSegmentId, null);
});

test('Heatmap: state resets when ledger_read_bytes changes', () => {
  const hm = createHeatmapState(BASE_COST);
  hm.setHoveredKey('auth_signatures');

  hm.resetIfNewContract({ ...BASE_COST, ledger_read_bytes: 80_000 });
  assert.equal(hm.hoveredKey, null);
});

test('Heatmap: state resets when ledger_write_bytes changes', () => {
  const hm = createHeatmapState(BASE_COST);
  hm.setActiveTab('hotspot');

  hm.resetIfNewContract({ ...BASE_COST, ledger_write_bytes: 50_000 });
  assert.equal(hm.activeTab, 'gauges');
});

test('Heatmap: state resets when transaction_size_bytes changes', () => {
  const hm = createHeatmapState(BASE_COST);
  hm.setHoveredRamId('ram-5');

  hm.resetIfNewContract({ ...BASE_COST, transaction_size_bytes: 10_000 });
  assert.equal(hm.hoveredRamId, null);
});

test('Heatmap: loading the same contract twice does not reset user hover state', () => {
  const hm = createHeatmapState(BASE_COST);
  hm.setActiveTab('ram');
  hm.setHoveredRamId('ram-0');

  // Simulate two identical rerender calls with the same cost
  hm.resetIfNewContract({ ...BASE_COST });
  hm.resetIfNewContract({ ...BASE_COST });

  assert.equal(hm.activeTab, 'ram');
  assert.equal(hm.hoveredRamId, 'ram-0');
});

// ── ResourceHeatmap – tab keyboard navigation ─────────────────────────────────

test('Heatmap tabs: ArrowRight wraps through all tabs', () => {
  const hm = createHeatmapState(BASE_COST);
  // gauges → ram → hotspot → footprint → gauges (wrap)
  hm.tabKeyDown('ArrowRight');
  assert.equal(hm.activeTab, 'ram');
  hm.tabKeyDown('ArrowRight');
  assert.equal(hm.activeTab, 'hotspot');
  hm.tabKeyDown('ArrowRight');
  assert.equal(hm.activeTab, 'footprint');
  hm.tabKeyDown('ArrowRight');
  assert.equal(hm.activeTab, 'gauges'); // wraps
});

test('Heatmap tabs: ArrowLeft wraps backwards', () => {
  const hm = createHeatmapState(BASE_COST);
  hm.tabKeyDown('ArrowLeft'); // gauges → footprint (wrap)
  assert.equal(hm.activeTab, 'footprint');
  hm.tabKeyDown('ArrowLeft');
  assert.equal(hm.activeTab, 'hotspot');
});

test('Heatmap tabs: Home jumps to first tab', () => {
  const hm = createHeatmapState(BASE_COST);
  hm.setActiveTab('footprint');
  hm.tabKeyDown('Home');
  assert.equal(hm.activeTab, 'gauges');
});

test('Heatmap tabs: End jumps to last tab', () => {
  const hm = createHeatmapState(BASE_COST);
  hm.tabKeyDown('End');
  assert.equal(hm.activeTab, 'footprint');
});

// ── FunctionSidebar keyboard navigation ──────────────────────────────────────

test('FunctionSidebar: ArrowDown moves selection to next item', () => {
  const state = createFunctionSidebarState(FUNCTIONS, 'transfer');
  state.handleKeyDown('ArrowDown', 0);
  assert.equal(state.selected.name, 'approve');
});

test('FunctionSidebar: ArrowDown wraps from last to first item', () => {
  const state = createFunctionSidebarState(FUNCTIONS, 'mint');
  state.handleKeyDown('ArrowDown', FUNCTIONS.length - 1);
  assert.equal(state.selected.name, 'transfer');
});

test('FunctionSidebar: ArrowUp moves selection to previous item', () => {
  const state = createFunctionSidebarState(FUNCTIONS, 'approve');
  state.handleKeyDown('ArrowUp', 1);
  assert.equal(state.selected.name, 'transfer');
});

test('FunctionSidebar: ArrowUp wraps from first to last item', () => {
  const state = createFunctionSidebarState(FUNCTIONS, 'transfer');
  state.handleKeyDown('ArrowUp', 0);
  assert.equal(state.selected.name, 'mint');
});

test('FunctionSidebar: Home jumps to first item', () => {
  const state = createFunctionSidebarState(FUNCTIONS, 'balance');
  state.handleKeyDown('Home', 2);
  assert.equal(state.selected.name, 'transfer');
});

test('FunctionSidebar: End jumps to last item', () => {
  const state = createFunctionSidebarState(FUNCTIONS, 'transfer');
  state.handleKeyDown('End', 0);
  assert.equal(state.selected.name, 'mint');
});

test('FunctionSidebar: Enter key does not change selection index', () => {
  const state = createFunctionSidebarState(FUNCTIONS, 'balance');
  const idx = state.handleKeyDown('Enter', 2);
  assert.equal(idx, 2);
  assert.equal(state.selected.name, 'balance');
});

test('FunctionSidebar: Space key does not change selection index', () => {
  const state = createFunctionSidebarState(FUNCTIONS, 'approve');
  const idx = state.handleKeyDown(' ', 1);
  assert.equal(idx, 1);
});

// ── NetworkSwitcher keyboard navigation ──────────────────────────────────────

test('NetworkSwitcher: dropdown opens and auto-focuses active network', () => {
  const sw = createNetworkSwitcherState(NETWORKS, 'testnet');
  sw.open();
  assert.equal(sw.isOpen, true);
  // testnet is index 1
  assert.equal(sw.focusedIdx, 1);
});

test('NetworkSwitcher: ArrowDown moves focus to next option', () => {
  const sw = createNetworkSwitcherState(NETWORKS, 'testnet');
  sw.open(); // focusedIdx = 1 (testnet)
  sw.handleKeyDown('ArrowDown');
  assert.equal(sw.focusedIdx, 2); // futurenet
});

test('NetworkSwitcher: ArrowDown wraps from last to first', () => {
  const sw = createNetworkSwitcherState(NETWORKS, 'localhost');
  sw.open(); // focusedIdx = 3
  sw.handleKeyDown('ArrowDown');
  assert.equal(sw.focusedIdx, 0); // mainnet
});

test('NetworkSwitcher: ArrowUp moves focus to previous option', () => {
  const sw = createNetworkSwitcherState(NETWORKS, 'futurenet');
  sw.open(); // focusedIdx = 2
  sw.handleKeyDown('ArrowUp');
  assert.equal(sw.focusedIdx, 1); // testnet
});

test('NetworkSwitcher: ArrowUp wraps from first to last', () => {
  const sw = createNetworkSwitcherState(NETWORKS, 'mainnet');
  sw.open(); // focusedIdx = 0
  sw.handleKeyDown('ArrowUp');
  assert.equal(sw.focusedIdx, NETWORKS.length - 1);
});

test('NetworkSwitcher: Home moves focus to first option', () => {
  const sw = createNetworkSwitcherState(NETWORKS, 'localhost');
  sw.open(); // focusedIdx = 3
  sw.handleKeyDown('Home');
  assert.equal(sw.focusedIdx, 0);
});

test('NetworkSwitcher: End moves focus to last option', () => {
  const sw = createNetworkSwitcherState(NETWORKS, 'mainnet');
  sw.open(); // focusedIdx = 0
  sw.handleKeyDown('End');
  assert.equal(sw.focusedIdx, NETWORKS.length - 1);
});

test('NetworkSwitcher: Escape closes the dropdown', () => {
  const sw = createNetworkSwitcherState(NETWORKS, 'testnet');
  sw.open();
  assert.equal(sw.isOpen, true);
  sw.handleKeyDown('Escape');
  assert.equal(sw.isOpen, false);
});

test('NetworkSwitcher: Enter selects the focused option and closes dropdown', () => {
  const sw = createNetworkSwitcherState(NETWORKS, 'testnet');
  sw.open(); // focusedIdx = 1
  sw.handleKeyDown('ArrowDown'); // move to futurenet (idx 2)
  sw.handleKeyDown('Enter');
  assert.equal(sw.activeId, 'futurenet');
  assert.equal(sw.isOpen, false);
});

test('NetworkSwitcher: keyboard events are no-ops when dropdown is closed', () => {
  const sw = createNetworkSwitcherState(NETWORKS, 'testnet');
  // dropdown is closed, so keys should not change focus or active
  sw.handleKeyDown('ArrowDown');
  sw.handleKeyDown('ArrowUp');
  assert.equal(sw.isOpen, false);
  assert.equal(sw.focusedIdx, -1);
  assert.equal(sw.activeId, 'testnet');
});
