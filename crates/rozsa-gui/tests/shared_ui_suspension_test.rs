// FrameworkTree
// shared_ui_suspension_test.rs
// ├── check_runtime()
// ├── suspended_mouse_slash_native_and_selection_paths_do_not_call_ipc_or_change_draft()
// ├── late_autocomplete_picker_and_panel_results_cannot_restore_suspended_ui()
// ├── running_stop_preserves_draft_and_passive_queue_state_while_keyboard_send_is_paused()
// └── idle_submit_and_slash_prefix_remain_available()

use std::process::Command;

fn check_runtime(checks: &str) {
    let harness = r#"
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const events = {};
const input = {textContent:'未发送 draft', style:{},addEventListener(){}};
const popup = {innerHTML:'', hidden:true, dataset:{}, classList:{add(){throw Error('old popup opened');},remove(){}},replaceChildren(){this.innerHTML='';}};
const elements = {msgInput:input,autocomplete:popup,forkPicker:popup,subagentPanel:popup};
elements.composer={addEventListener(name,fn){events['composer-'+name]=fn;}};
const document = {documentElement:{addEventListener(){}},addEventListener(name,fn){(events[name]??=[]).push(fn);},
  getElementById(id){return elements[id]??null;},querySelector(){return null;}};
const calls=[];
const context=vm.createContext({document,window:{addEventListener(){}},console,performance:{now(){return 5000;}},setTimeout,clearTimeout,assert,calls});
vm.runInContext(fs.readFileSync(process.argv[1],'utf8'),context);
vm.runInContext(`invoke=async command=>{calls.push(command);throw Error('unexpected IPC: '+command);};`,context);
async function run(code){return vm.runInContext('(async()=>{'+code+'})()',context);}
"#;
    let script = format!(
        "{harness}\n(async()=>{{{checks}}})().catch(error=>{{console.error(error);process.exitCode=1;}});"
    );
    let output = Command::new("node")
        .args([
            "-e",
            &script,
            concat!(env!("CARGO_MANIFEST_DIR"), "/frontend/app.js"),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn suspended_mouse_slash_native_and_selection_paths_do_not_call_ipc_or_change_draft() {
    check_runtime(
        r#"
await run(`
  await attachFileReference();await attachDirectoryReference();await attachReference('file');
  insertFileReferences(['/tmp/file']);handleNativeFileDrag('enter');handleNativeFileDrag('drop',{paths:['/tmp/file']});
  await updateAutocomplete();acHighlight(1);selectAutocomplete(0);
  assert.equal(navigateAutocomplete('down'),false);assert.equal(confirmAutocomplete(),false);
  await showModelPicker();selectModel(0);await showForkPicker();await forkAtMessage(0);await showSubagentPanel();
  await handleSlashAction('modelPicker');await handleSlashAction('forkPicker');await handleSlashAction('subagentPanel');
  await dispatchSlashCommand('/model actual-model');await dispatchSlashCommand('/fork');await dispatchSlashCommand('/subagents');
  isStreaming=true;await sendMessage();
`);
vm.runInContext(fs.readFileSync(process.argv[1].replace('/app.js','/ui/scripts/components/composer.js'),'utf8').replaceAll('export ',''),context);
await run(`init({boundModules:new Set(),byId:id=>document.getElementById(id),attachFileReference,attachDirectoryReference,insertSlashCommandPrefix,showModelPicker,addMessage:sendMessage,updateAbortButton});`);
for(const action of ['attach-file','attach-directory','model-picker','send']) {
  events['composer-click']({target:{closest(){return {dataset:{action}};}}});
}
assert.equal(input.textContent,'未发送 draft');assert.equal(popup.innerHTML,'');assert.equal(popup.hidden,true);assert.deepEqual(calls,[]);
await run(`assert.equal(suspendedConversationCommand('a /model model-id'),true);assert.equal(suspendedConversationCommand('/MODEL'),true);assert.equal(suspendedConversationCommand('/model-name'),false);assert.equal(suspendedConversationCommand('https://example.test/model'),false);`);
"#,
    );
}

#[test]
fn late_autocomplete_picker_and_panel_results_cannot_restore_suspended_ui() {
    check_runtime(
        r#"
for(const [operation,start,result] of [
  ['attachments',"attachReference('file')",'/tmp/late'],
  ['autocomplete','updateAutocomplete()',{items:[{value:'/late'}],prefix:'/',highlightRanges:[]}],
  ['modelPicker','showModelPicker()',[{id:'late'}]],
  ['fork','showForkPicker()',[{messageIndex:0,label:'late'}]],
  ['subagents','showSubagentPanel()',[{name:'late'}]],
]) {
  let release;
  context.pending=new Promise(resolve=>{release=resolve;});
  await run(`conversationOperationSuspended=()=>false;invoke=()=>pending;getInputText=input=>input.textContent;getInputCursor=()=>0;renderModelSelector=()=>{};`);
  const pending=run('await '+start+';');
  await run('conversationOperationSuspended=operation=>Object.hasOwn(conversationSuspensions,operation);');
  release(result);await pending;
  assert.equal(input.textContent,'未发送 draft',operation);assert.equal(popup.innerHTML,'',operation);
}
"#,
    );
}

#[test]
fn running_stop_preserves_draft_and_passive_queue_state_while_keyboard_send_is_paused() {
    check_runtime(
        r#"
const button={dataset:{},style:{},setAttribute(){}};
const mode={hidden:false};
document.querySelector=selector=>selector.includes('send-btn')?button:null;
elements.runningSendMode=mode;
context.window.RozsaComposer={renderSendState(button,action){button.dataset.sendAction=action;}};
await run('isStreaming=true;updateAbortButton();');
assert.equal(button.dataset.sendAction,'stop');assert.equal(mode.hidden,true);
// Existing runtime keyboard dispatch reaches the same guarded send handler.
context.keyEvent={key:'Enter',target:input,preventDefault(){}};
await run('notificationErrorList=()=>({hidden:true});');
document.activeElement=input;
events.keydown[0](context.keyEvent);
events.keydown[0]({key:'p',ctrlKey:true,target:input,preventDefault(){}});
await new Promise(resolve=>setImmediate(resolve));
assert.equal(input.textContent,'未发送 draft');
assert.deepEqual(calls,[]);
const entry=fs.readFileSync(process.argv[1].replace('/app.js','/runtime/entries/main.js'),'utf8');
const submit=entry.match(/submit: (.*),/)[1];
context.stops=0;
await run('abortAgent=async()=>{stops++;};await ('+submit+')();');
assert.equal(context.stops,1);assert.equal(input.textContent,'未发送 draft');
const panels={queuedMessages:{hidden:true},steeringConversation:{hidden:true}};
Object.assign(elements,panels);
context.window.RozsaConversation={runningPanelMarkup(panel){return JSON.stringify(panel);}};
await run(`renderRunningMessages(['existing queue'],[{text:'existing steer'}]);`);
assert.match(panels.queuedMessages.innerHTML,/existing queue/);
assert.match(panels.steeringConversation.innerHTML,/existing steer/);
await run(`renderRunningMessages([],[]);`);
assert.equal(panels.queuedMessages.hidden,true);assert.equal(panels.steeringConversation.hidden,true);
"#,
    );
}

#[test]
fn idle_submit_and_slash_prefix_remain_available() {
    check_runtime(
        r#"
context.sent=[];
await run(`
  nativeSplitMode=true;
  getInputText=input=>input.textContent;
  setInputText=(input,text)=>{input.textContent=text;};
  updateInputHighlight=()=>{};
  invoke=async(command,args)=>{sent.push([command,args]);};
  await sendMessage();
`);
assert.deepEqual(context.sent.map(([command,args])=>[command,args.message]),[['send_message','未发送 draft']]);
assert.equal(input.textContent,'');
await run(`insertInputText=text=>{document.getElementById('msgInput').textContent+=text;};insertSlashCommandPrefix();`);
assert.equal(input.textContent,'/');
input.textContent='/model selected';
await run('await sendMessage();');
assert.equal(context.sent.length,1);assert.equal(input.textContent,'/model selected');
"#,
    );
}
