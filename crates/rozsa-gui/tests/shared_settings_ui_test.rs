// FrameworkTree
// shared_settings_ui_test.rs
// └── settings_share_dispatch_and_suspend_uncovered_routes_and_mutations()

use std::process::Command;

#[test]
fn settings_share_dispatch_and_suspend_uncovered_routes_and_mutations() {
    let script = r#"
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const base = process.argv[1];
const events = {}, calls = [], panes = {}, buttons = {};
const classes = () => ({add(){},remove(){},toggle(){}});
for(const name of ['general','skills','tools','keyboard-shortcuts','permissions']) {
  panes[name] = {dataset:{},children:['old'],replaceChildren(){this.children=[];}};
  buttons[name] = {disabled:false,setAttribute(name,value){this[name]=value;}};
}
const range={min:'5',max:'30',value:'14',style:{setProperty(name,value){this[name]=value;}}};
const input={value:'14'};
const panel={addEventListener(name,callback){(events[name]??=[]).push(callback);}};
const document={documentElement:{addEventListener(){}},addEventListener(){},
  querySelector(selector){if(selector.startsWith('#pane-'))return panes[selector.slice(6)];if(selector==='#settingsFontSizeRange')return range;if(selector==='#settingsFontSizeInput')return input;return null;},
  querySelectorAll(selector){const match=selector.match(/data-settings-pane="([^"]+)"/);return match ? [buttons[match[1]]].filter(Boolean) : [];},
  getElementById(id){return id==='settingsPanel'?panel:null;}};
const window={addEventListener(){}};
const context=vm.createContext({document,window,console,assert,calls,setTimeout,clearTimeout,ResizeObserver:class {observe(){}}});
vm.runInContext(fs.readFileSync(base+'/ui/scripts/features/settings.js','utf8').replaceAll('export ',''),context);
vm.runInContext('window.RozsaSettings={isSettingsPaneAvailable,syncFontSizeControls,renderThemeModeCards};',context);
vm.runInContext(fs.readFileSync(base+'/runtime/adapters/settings.js','utf8').replace(/^import .*;\n/m,'').replaceAll('export ',''),context);
vm.runInContext(`window.RozsaSettings.bindSettingSwitch=bindSettingSwitch;
init({...createSettingsAdapter({switchSettingsTab:pane=>calls.push(pane),closeSettings(){},selectThemeModeCard:mode=>calls.push(mode),previewFontSize:value=>calls.push(value),commitFontSize:value=>calls.push(value)}),boundModules:new Set(),byId:()=>document.getElementById('settingsPanel')});`,context);
for(const name of Object.keys(panes)){assert.deepEqual(panes[name].children,[]);assert.equal(buttons[name].disabled,true);}
const target=node=>({closest(selector){return selector==='.settings-tab'?node:null;}});
const click = event => events.click.forEach(callback=>callback(event));
click({target:target({disabled:false,dataset:{settingsPane:'permissions'}})});
assert.deepEqual(calls,[]);
click({target:target({disabled:false,dataset:{settingsPane:'extensions'}})});
assert.deepEqual(calls,['extensions']);calls.length=0;
const toggle={disabled:false,checked:false,onclick(){assert.fail('old direct binding remained');}};
context.toggle=toggle;
vm.runInContext('bindSettingSwitch(toggle,enabled=>calls.push(enabled),control=>control.checked,(control,enabled)=>control.checked=enabled);',context);
assert.equal(toggle.onclick,null);
click({target:{closest(selector){return selector==='.setting-toggle'?toggle:null;}}});
assert.deepEqual(calls,[true]);toggle.disabled=true;
click({target:{closest(selector){return selector==='.setting-toggle'?toggle:null;}}});
assert.deepEqual(calls,[true]);calls.length=0;
vm.runInContext(fs.readFileSync(base+'/app.js','utf8'),context);
vm.runInContext(`invoke=async command=>{calls.push(command);throw Error('unexpected IPC');};`,context);
(async()=>{
  await vm.runInContext(`(async()=>{
    nativeSplitMode=true;
    await requestGuiScene('settings','general');switchSettingsTab('skills');
    renderGeneralSettings({});renderCapabilityPane('tools',[{}]);renderPermissionSettings();renderPermissionRuleList('deny');
    openPermissionRuleEditor('deny');await savePermissionRule();await replacePermissionRules('deny',[]);await movePermissionRule('deny','allow','Bash(*)');
    renderKeyBindings();beginKeyBindingCapture('openSettings');await resetKeyBinding('openSettings');
    assert.equal(await saveCapturedKeyBinding({}),false);
  })()`,context);
  assert.deepEqual(calls,[],'paused routes, renderers and late mutations must stay inert');
  vm.runInContext(`invoke=async(command,payload)=>{calls.push([command,payload]);return command==='get_settings'?{appearance:{fontSize:18}}:[];};
    renderSettingsPane=()=>{};applySelectedTheme=async()=>{};loadDevFlowSettings=()=>{};applyFontSize=value=>calls.push(['font',value]);`,context);
  await vm.runInContext('loadSettings()',context);
  assert.deepEqual(calls.map(call=>call[0]),['get_settings','list_themes']);calls.length=0;
  input.value='';
  vm.runInContext("previewFontSize('')",context);
  assert.equal(input.value,'','preview must preserve an empty number edit');
  input.value='1';
  vm.runInContext("previewFontSize('1')",context);
  assert.equal(input.value,'1','preview must preserve the first digit of 18');
  calls.length=0;input.value='18';
  vm.runInContext('previewFontSize(18)',context);
  assert.equal(range.value,'18');assert.equal(input.value,'18');assert.equal(range.style['--appearance-progress'],'52%');
  assert.deepEqual(JSON.parse(JSON.stringify(calls)),[['font',18]]);calls.length=0;
  await vm.runInContext('commitFontSize(19)',context);
  assert.deepEqual(JSON.parse(JSON.stringify(calls[0])),['update_setting',{key:'appearance_font_size',value:'19'}]);
  assert.deepEqual(calls.slice(1).map(call=>call[0]),['get_settings','list_themes']);
  calls.length=0;
  vm.runInContext(fs.readFileSync(base+'/sidebar.js','utf8'),context);
  await vm.runInContext(`(async()=>{sidebarInvoke=async()=>{throw Error('unexpected sidebar IPC');};await requestSidebarScene('settings','permissions');selectSidebarSettingsPane('tools');})()`,context);
  assert.deepEqual(calls,[]);
})().catch(error=>{console.error(error);process.exitCode=1;});
"#;
    let output = Command::new("node")
        .args([
            "-e",
            script,
            concat!(env!("CARGO_MANIFEST_DIR"), "/frontend"),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
