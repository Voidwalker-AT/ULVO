// Synthetic IPC fixtures only; no host discovery, provisioning or vault writes.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Module = require('node:module');
const ts = require('typescript');
const { renderToStaticMarkup } = require('react-dom/server');
const React = require('react');
const file = path.resolve(__dirname, '../src/AdoptionView.tsx');
const compiled = ts.transpileModule(fs.readFileSync(file, 'utf8'), { compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 } }).outputText;
const mod = new Module(file, module); mod.filename = file; mod.paths = module.paths; mod._compile(compiled, file);
const { adoptionController, AdoptionContent } = mod.exports;
const empty = { installations: [], total_discovered: 0, managed_count: 0, adoptable_count: 0 };
function harness(allowed = false, report = empty) {
  const calls = []; const states = []; let permission = allowed;
  const invoke = async (name, args) => { calls.push(name); if (name === 'get_discovery_permission') return permission; if (name === 'set_discovery_permission') { permission = args.allowed; return; } return report; };
  return { invoke, calls, states, controller: adoptionController(invoke, s => states.push(s)), current: () => states.at(-1) };
}
const html = state => renderToStaticMarkup(React.createElement(AdoptionContent, { state, permission() {}, retry() {}, installed() {}, adopt() {} }));
test('opt-out renders disabled state and never invokes discovery', async () => { const h = harness(); await h.controller.load(); assert.match(html(h.current()), /Discovery is disabled/); assert.deepEqual(h.calls, ['get_discovery_permission']); });
test('enabled discovery renders actual IPC results including managed records', async () => { const h = harness(true, { ...empty, installations: [{ name: 'Synthetic registered tool', path: 'fixture', classification: 'managed', explanation: 'fixture', ecosystem: 'python' }] }); await h.controller.load(); assert.match(html(h.current()), /Synthetic registered tool/); assert.match(html(h.current()), /managed/); });
test('zero results render an explicit empty state', async () => { const h = harness(true); await h.controller.load(); assert.match(html(h.current()), /No tools found/); });
test('IPC failure renders actionable error, not an empty main pane', async () => { const states = []; const c = adoptionController(async name => { if (name === 'get_discovery_permission') return true; throw new Error('synthetic IPC unavailable'); }, s => states.push(s)); await c.load(); assert.match(html(states.at(-1)), /synthetic IPC unavailable/); assert.match(html(states.at(-1)), /Refresh discovery/); });
test('pending discovery displays loading state', async () => { let resolve; const states = []; const c = adoptionController(name => name === 'get_discovery_permission' ? Promise.resolve(true) : new Promise(r => { resolve = r; }), s => states.push(s)); const pending = c.load(); await Promise.resolve(); await Promise.resolve(); assert.match(html(states.at(-1)), /Loading discovery state/); while (!resolve) await new Promise(r => setImmediate(r)); resolve(empty); await pending; });
test('permission changes persist only after explicit action', async () => { const h = harness(false); await h.controller.load(); await h.controller.permission(true); assert.equal(h.current().allowed, true); await h.controller.permission(false); assert.equal(h.current().allowed, false); assert.match(html(h.current()), /Discovery is disabled/); });
test('navigation unmount ignores late replies; return reloads consent', async () => { let resolve; const states = []; const c = adoptionController(() => new Promise(r => { resolve = r; }), s => states.push(s)); const p = c.load(); c.dispose(); const count = states.length; resolve(true); await p; assert.equal(states.length, count); const h = harness(false); await h.controller.load(); assert.match(html(h.current()), /View registered tools/); });
test('relaunch reads persisted preference instead of enabling by default', async () => { const h = harness(false); await h.controller.load(); await h.controller.permission(true); const states = []; const relaunched = adoptionController(h.invoke, s => states.push(s)); await relaunched.load(); assert.equal(states.at(-1).allowed, true); });
test('failed permission write preserves prior choice', async () => { const states = []; const c = adoptionController(async name => { if (name === 'get_discovery_permission') return false; throw new Error('synthetic write denied'); }, s => states.push(s)); await c.load(); await c.permission(true); assert.equal(states.at(-1).allowed, false); assert.match(html(states.at(-1)), /Permission was not changed/); });
