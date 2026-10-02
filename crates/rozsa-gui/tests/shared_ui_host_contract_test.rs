// FrameworkTree
// shared_ui_host_contract_test.rs
// ├── shared_ports_preserve_data_errors_cancellation_and_revision()
// ├── both_hosts_reference_the_same_ui_without_production_mock_dependencies()
// ├── repository_root()
// ├── shared_popover_positions_in_css_units_under_host_zoom()
// ├── native_ui_entry_exposes_readiness_before_delayed_template_loading()
// └── native_settings_sidebar_publishes_readiness_before_navigation_mount()

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn shared_ports_preserve_data_errors_cancellation_and_revision() {
    let root = repository_root();
    let source = r#"
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
const moduleUrl = source => 'data:text/javascript;base64,' + Buffer.from(source).toString('base64');
const ports = moduleUrl(await readFile('crates/rozsa-gui/frontend/ui/scripts/core/ports.js', 'utf8'));
const renderer = (await readFile('crates/rozsa-gui/frontend/ui/scripts/core/renderer.js', 'utf8')).replace('./ports.js', ports);
const { createUIHost } = await import(moduleUrl(renderer));
const { createTauriAdapter } = await import(moduleUrl(await readFile('crates/rozsa-gui/frontend/runtime/adapters/tauri.js', 'utf8')));
let receive, removed = 0, calls = [];
const failure = new Error('product rejected');
const host = createUIHost(createTauriAdapter(() => ({
  core: { invoke: async (command, data) => {
    calls.push([command, data]);
    if (command === 'fail') throw failure;
    if (command === 'pending') return new Promise(() => {});
    return data;
  } },
  event: { listen: async (event, callback) => {
    receive = callback;
    return () => { removed++; };
  } },
})));
assert.deepEqual(await host.port.request('get_state', { value: 7 }), { value: 7 });
await assert.rejects(host.port.request('fail'), error => error === failure);
let delivered = [];
const unsubscribe = await host.port.subscribe('ui-state', payload => delivered.push(payload));
receive({ payload: { revision: 1 } });
unsubscribe(); unsubscribe();
receive({ payload: { revision: 2 } });
assert.deepEqual(delivered, [{ revision: 1 }]);
assert.equal(removed, 1);
const cancel = new AbortController();
const pending = host.port.request('pending', null, { signal: cancel.signal });
await Promise.resolve();
cancel.abort();
await assert.rejects(pending, { name: 'AbortError' });
assert(!calls.some(([command]) => command === 'abort'), 'delivery cancellation must not invent product abort');
const preCancelled = new AbortController(); preCancelled.abort();
const count = calls.length;
await assert.rejects(host.port.request('never', null, { signal: preCancelled.signal }), { name: 'AbortError' });
assert.equal(calls.length, count);
const state = { revision: 1 };
const valid = snapshot => snapshot.scene === 'main';
assert.equal(host.applyRevision(state, { revision: 1, scene: 'main' }, valid, () => assert.fail()), false);
assert.equal(host.applyRevision(state, { revision: 2, scene: 'unknown' }, valid, () => assert.fail()), false);
assert.throws(() => host.applyRevision(state, { revision: 2, scene: 'main' }, valid, () => { throw failure; }));
assert.equal(state.revision, 1);
assert.equal(host.applyRevision(state, { revision: 2, scene: 'main' }, valid, () => {}), true);
assert.equal(state.revision, 2);
await host.port.subscribe('theme-state', () => {});
const disposal = host.port.request('pending');
host.port.dispose(); host.port.dispose();
await assert.rejects(disposal, { name: 'AbortError' });
await assert.rejects(host.port.request('never'), { name: 'AbortError' });
assert.equal(removed, 2);
// Registration resolving after disposal must detach without delivering events.
let resolveRegistration, lateRemoved = 0;
const late = createUIHost({ request() {}, subscribe(event, callback) {
  callback('before');
  return new Promise(resolve => { resolveRegistration = resolve; });
} });
const registration = late.port.subscribe('late', () => {});
late.port.dispose();
resolveRegistration(() => { lateRemoved++; });
await assert.rejects(registration, { name: 'AbortError' });
assert.equal(lateRemoved, 1);
"#;
    let output = Command::new("node")
        .args(["--input-type=module", "-e", source])
        .current_dir(root)
        .output()
        .expect("node is required for shared UI contract verification");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn both_hosts_reference_the_same_ui_without_production_mock_dependencies() {
    let root = repository_root();
    for webview in ["main", "sidebar"] {
        let entry = fs::read_to_string(root.join(format!(
            "crates/rozsa-gui/frontend/runtime/entries/{webview}.js"
        )))
        .unwrap();
        assert!(entry.contains("../../ui/scripts/core/renderer.js"));
        assert!(entry.contains("createTauriAdapter"));
        let html = if webview == "main" {
            "index"
        } else {
            "sidebar"
        };
        let html = fs::read_to_string(root.join(format!("crates/rozsa-gui/frontend/{html}.html")))
            .unwrap();
        assert!(html.contains(&format!(
            "type=\"module\" src=\"runtime/entries/{webview}.js\""
        )));
    }
    let shell = fs::read_to_string(root.join("prototype/scripts/core/shell.js")).unwrap();
    assert!(shell.contains("crates/rozsa-gui/frontend/ui/scripts/core/renderer.js"));
    assert!(shell.contains("createMockAdapter"));
    for path in ["ports", "renderer"] {
        let source = fs::read_to_string(root.join(format!(
            "crates/rozsa-gui/frontend/ui/scripts/core/{path}.js"
        )))
        .unwrap();
        assert!(!source.contains("__TAURI__"));
        assert!(!source.contains("prototype/mock"));
    }
    let adapter =
        fs::read_to_string(root.join("crates/rozsa-gui/frontend/runtime/adapters/tauri.js"))
            .unwrap();
    assert!(!adapter.contains("document."));
    assert!(!adapter.contains("mock"));
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}

#[test]
fn shared_popover_positions_in_css_units_under_host_zoom() {
    let source = r#"
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
const code = await readFile('crates/rozsa-gui/frontend/ui/scripts/components/popover.js', 'utf8');
const { init } = await import('data:text/javascript;base64,' + Buffer.from(code).toString('base64'));
globalThis.window = { innerWidth: 896, innerHeight: 720 };
const scale = 16 / 14;
const popover = { hidden: false, style: {}, offsetWidth: 320,
  getBoundingClientRect: () => ({ width: 320 * scale, height: 106 * scale }) };
const trigger = { getBoundingClientRect: () => ({ left: 790, bottom: 694 }),
  addEventListener() {}, setAttribute() {} };
const slider = { addEventListener() {} };
const elements = { thinkingEffortPopover: popover, thinkingEffort: trigger, thinkingEffortSlider: slider };
const controller = init({ boundModules: new Set(), byId: id => elements[id] });
controller.position();
const left = parseFloat(popover.style.left) * scale;
const top = parseFloat(popover.style.top) * scale;
assert(left + 320 * scale <= window.innerWidth, 'popover must fit horizontally after zoom');
assert(top + 106 * scale <= window.innerHeight, 'popover must fit vertically after zoom');
"#;
    let output = Command::new("node")
        .args(["--input-type=module", "-e", source])
        .current_dir(repository_root())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_ui_entry_exposes_readiness_before_delayed_template_loading() {
    let source = r#"
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
const url = source => 'data:text/javascript;base64,' + Buffer.from(source).toString('base64');
const modules = {
  '../../ui/scripts/core/renderer.js': 'export function createUIHost(){return {port:{dispose(){}}};}',
  '../adapters/tauri.js': 'export function createTauriAdapter(){return {};}',
  '../../ui/scripts/core/shell.js': 'export const templateRoot=new URL("https://fixture.invalid/");export async function mountShell(){}',
  '../../ui/scripts/components/popover.js': 'export function init(){return {mounted:true};}',
  '../../ui/scripts/components/composer.js': 'export function init(){};export function renderSendState(){}',
  '../../ui/scripts/features/conversation.js': 'export function init(){};export function emptyConversationMarkup(){};export function messageFrame(){};export function createMessageFrame(){};export function runningPanelMarkup(){};export function thinkingMarkup(){}',
  '../../ui/scripts/flows/conversation.js': 'export function mountConversation(){globalThis.shellStarted();return globalThis.shellPending;}export function createConversationFlow(){return {};}',
  '../adapters/conversation.js': 'export function createConversationAdapter(){return {};}',
  '../../ui/scripts/features/tools.js': 'export async function loadToolTemplate(){}',
  '../../ui/scripts/components/tool-call.js': 'export function init(){}',
  '../adapters/tools.js': 'export function toolView(){} export function activityView(){}',
  '../../ui/scripts/features/permissions.js': 'export function questionOptionsMarkup(){}',
  '../../ui/scripts/components/interruption-panel.js': 'export function init(){}',
  '../../ui/scripts/flows/composer-interruption.js': 'export function init(){}',
  '../adapters/interruption.js': 'export function createInterruptionAdapter(){return {};}',
  '../../ui/scripts/features/settings.js': 'export function init(){}',
  '../adapters/settings.js': 'export function bindSettingSwitch(){} export function createSettingsAdapter(){return {};}',
};
let code = await readFile('crates/rozsa-gui/frontend/runtime/entries/main.js', 'utf8');
for(const [path, module] of Object.entries(modules)) code = code.replace(path,url(module));
globalThis.window={addEventListener(){}};
globalThis.previewThinkingEffort=globalThis.selectThinkingEffort=()=>{};
globalThis.toggleToolCall=globalThis.toggleTurnDiff=()=>{};
for(const name of ['switchSettingsTab','closeSettings','selectThemeModeCard','previewFontSize','commitFontSize'])globalThis[name]=()=>{};
for(const name of ['configureInterruptionSuspensions','respondPermission','enterPermissionTrust','enterPermissionHint','togglePermissionCommand','showPermissionMainPage','submitPermissionHint','submitUserQuestion','normalizePermissionHint','handlePermissionHintKeydown'])globalThis[name]=()=>{};
for(const name of ['configureConversationSuspensions','attachFileReference','attachDirectoryReference','insertSlashCommandPrefix','showModelPicker','updateAbortButton','handleInput','handleCompositionStart','handleCompositionUpdate','handleCompositionEnd','toggleThinking'])globalThis[name]=()=>{};
const element={content:{cloneNode(){},querySelector(){return {cloneNode(){return {};}};}},replaceWith(){},prepend(){}};
globalThis.document={getElementById(){return element;},querySelector(){return element;}};
let release, started;
globalThis.shellPending=new Promise(resolve=>{release=resolve;});
const mounting=new Promise(resolve=>{globalThis.shellStarted=resolve;});
const loading=import(url(code));
await mounting;
assert(window.RozsaUIReady instanceof Promise,'entry must publish a promise before mounting completes');
assert.equal(window.RozsaPopover,undefined);
release();
await window.RozsaUIReady;
await loading;
assert.equal(window.RozsaPopover.mounted,true);
const app=await readFile('crates/rozsa-gui/frontend/app.js','utf8');
const initialization=app.slice(app.indexOf("window.addEventListener('DOMContentLoaded'"));
assert(initialization.indexOf('await window.RozsaUIReady') < initialization.indexOf('preparePlatformSceneDom()'));
assert(initialization.includes("showError('Shared UI failed to load: '"));
"#;
    let output = Command::new("node")
        .args(["--input-type=module", "-e", source])
        .current_dir(repository_root())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_settings_sidebar_publishes_readiness_before_navigation_mount() {
    let source = r#"
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
const url = source => 'data:text/javascript;base64,' + Buffer.from(source).toString('base64');
let release, started, mounted=false, bound=false;
globalThis.pending=new Promise(resolve=>{release=resolve;});
const loadingStarted=new Promise(resolve=>{started=resolve;});
globalThis.templateStarted=()=>started();
globalThis.window={addEventListener(){}};
globalThis.selectSidebarSettingsPane=globalThis.closeSidebarSettings=()=>{};
const navigation={};
const template={querySelector(){return {content:{cloneNode(){return navigation;}}};}};
globalThis.readyTemplate=template;
globalThis.document={getElementById(id){assert.equal(id,'settingsSidebarScene');return {replaceChildren(value){assert.equal(value,navigation);mounted=true;}};}};
globalThis.bindNavigation=()=>{assert.equal(mounted,true);bound=true;};
const modules={
  '../../ui/scripts/core/renderer.js':'export function createUIHost(){return {port:{dispose(){}}};}',
  '../adapters/tauri.js':'export function createTauriAdapter(){return {};}',
  '../../ui/scripts/core/shell.js':'export const templateRoot=new URL("https://fixture.invalid/");export async function mountShell(){}',
  '../../ui/scripts/core/template-loader.js':'export async function loadTemplate(){globalThis.templateStarted();await globalThis.pending;return globalThis.readyTemplate;}',
  '../../ui/scripts/features/settings.js':'export function configureSettingsSuspensions(){} export function isSettingsPaneAvailable(){} export function bindSettingsNavigation(){globalThis.bindNavigation();}',
};
let code=await readFile('crates/rozsa-gui/frontend/runtime/entries/sidebar.js','utf8');
for(const [path,module] of Object.entries(modules))code=code.replace(path,url(module));
await import(url(code));await loadingStarted;
assert(window.RozsaUIReady instanceof Promise);
assert.equal(mounted,false);assert.equal(bound,false);
release();await window.RozsaUIReady;
assert.equal(mounted,true);assert.equal(bound,true);
const sidebar=await readFile('crates/rozsa-gui/frontend/sidebar.js','utf8');
const initialization=sidebar.slice(sidebar.indexOf("window.addEventListener('DOMContentLoaded'"));
assert(initialization.indexOf('await window.RozsaUIReady')<initialization.indexOf("sidebarInvoke ="));
assert(initialization.includes("renderSidebarError('Shared Settings UI failed to load: '"));
"#;
    let output = Command::new("node")
        .args(["--input-type=module", "-e", source])
        .current_dir(repository_root())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
