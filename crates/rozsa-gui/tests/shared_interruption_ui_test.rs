// FrameworkTree
// shared_interruption_ui_test.rs
// ├── run_node()
// ├── shared_question_validation_binding_and_dispatch_preserve_user_input()
// └── runtime_trust_is_suspended_and_real_responses_preserve_queue_and_failures()

use std::path::Path;
use std::process::Command;

fn run_node(source: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let output = Command::new("node")
        .args(["--input-type=module", "-e", source])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn shared_question_validation_binding_and_dispatch_preserve_user_input() {
    run_node(
        r#"
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
const load = async path => import('data:text/javascript;base64,' + Buffer.from(await readFile(path,'utf8')).toString('base64'));
const feature = await load('crates/rozsa-gui/frontend/ui/scripts/features/permissions.js');
const entry=await readFile('crates/rozsa-gui/frontend/runtime/entries/main.js','utf8');
assert(entry.includes('selector: "[data-template-mount=interruption-panel]", children: true'), 'panels must be direct composer children');
const template=await readFile('crates/rozsa-gui/frontend/ui/templates/composites/interruption-panel.html','utf8');
assert.equal((template.match(/class="perm-panel-opt"/g)||[]).length,4,'keep all four covered permission options');
const markup = feature.questionOptionsMarkup([{label:'<A>',description:'&',value:'a'}, {label:'Other',other:true,value:'other'}], 'request"a');
assert(markup.includes('&lt;A&gt;') && markup.includes('&amp;') && markup.includes('request&quot;a'));
assert(markup.includes('value="a"') && markup.includes('data-other="true"'));
const choices = [false,true].map(other=>({dataset:{other:String(other)},checked:false,value:other?'other':'a',addEventListener(event,fn){this[event]=fn;}}));
const options={querySelectorAll(){return choices;},querySelector(selector){return selector==='input:checked'?choices.find(c=>c.checked):choices[1];}};
const other={hidden:true,value:'',focus(){this.focused=true;}};
const error={textContent:''};
const messages={missing:'Choose',otherMissing:'Type'};
feature.bindQuestionOptions(options,other,error,{selectOtherOnInput:true});
assert.equal(feature.collectSingleQuestionAnswer(options,other,error,messages),null);
assert.equal(error.textContent,'Choose');
choices[1].checked=true;choices[1].change();
assert.equal(other.hidden,false);assert(other.focused);
assert.equal(feature.collectSingleQuestionAnswer(options,other,error,messages),null);
assert.equal(error.textContent,'Type');
other.value='  custom answer  ';other.oninput();
assert.equal(feature.collectSingleQuestionAnswer(options,other,error,messages),'custom answer');
choices[1].checked=false;choices[0].checked=true;choices[0].change();
assert(other.hidden);assert.equal(other.value,'');assert.equal(error.textContent,'');
assert.equal(feature.collectSingleQuestionAnswer(options,other,error,messages),'a');
let click, hintInput, hintKey, calls=[];
globalThis.document={querySelector(){return {addEventListener(type,fn){click=fn;}};}};
const adapter=(await load('crates/rozsa-gui/frontend/runtime/adapters/interruption.js')).createInterruptionAdapter({
 respondPermission:choice=>calls.push(choice),enterPermissionTrust:()=>calls.push('trust'),enterPermissionHint:()=>calls.push('hint'),
 togglePermissionCommand(){},showPermissionMainPage(){},submitPermissionHint:()=>calls.push('submit-hint'),
 submitUserQuestion:()=>calls.push('question'),normalizePermissionHint:value=>calls.push(value),handlePermissionHintKeydown:()=>calls.push('key')
});
const api={...adapter,boundModules:new Set(),byId(){return {addEventListener(type,fn){if(type==='input')hintInput=fn;else hintKey=fn;}};}};
const {init}=await load('crates/rozsa-gui/frontend/ui/scripts/components/interruption-panel.js');
init(api);init(api);
for(const action of ['allow','deny','hint','submit-hint','submit-question'])click({target:{closest(){return {dataset:{permissionAction:action}};}}});
hintInput({currentTarget:'input'});hintKey();
assert.deepEqual(calls,['allow','deny','hint','submit-hint','question','input','key']);
"#,
    );
}

#[test]
fn runtime_trust_is_suspended_and_real_responses_preserve_queue_and_failures() {
    run_node(
        r#"
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
const app=await readFile('crates/rozsa-gui/frontend/app.js','utf8');
const section=app.slice(app.indexOf('const QUESTION_OTHER_VALUE'),app.indexOf('// =============== Send Message'));
const elements=new Map();
function element(id){if(!elements.has(id))elements.set(id,{dataset:{},style:{},hidden:false,classList:{remove(){},add(){},contains(){return false;}},focus(){},replaceWith(slot){this.slot=slot;}});return elements.get(id);}
let calls=[], fail=false, errors=[], events=[];
const context={console:{error(){}},CustomEvent:class{constructor(type,options){this.type=type;this.detail=options.detail;}},
 document:{getElementById:element,querySelector(){return element('trust');},createElement(){return {dataset:{}};},dispatchEvent(event){events.push(event);}},
 pendingPermissions:{a:[{session_id:'a',request_id:'p1'},{session_id:'a',request_id:'p2'}],b:[{session_id:'b',request_id:'p1'}]},
 pendingUserQuestions:{a:[{sessionId:'a',requestId:'q1',questions:[{header:'Choice'}]}],b:[{sessionId:'b',requestId:'q1'}]},
 currentPermissionId:'p1',currentPermissionSessionId:'a',currentPermissionTrustGroups:[],currentPermissionTrustKeys:[],currentPermissionTrustIndex:-1,
 currentQuestionId:'q1',currentQuestionSessionId:'a',currentQuestionIndex:0,currentQuestionAnswers:{},
 sessionStreamingState:{},permissionUiStateBySession:{a:{requestId:'p1'}},
 invoke:async(command,payload)=>{calls.push([command,structuredClone(payload)]);if(fail)throw Error('rejected');},showError:message=>errors.push(message)
};
vm.createContext(context);vm.runInContext(section,context);
context.schedulePermPanelDisplay=context.scheduleQuestionPanelDisplay=()=>{};
context.configureInterruptionSuspensions();
assert.equal(element('trust').dataset.uiSuspended,'permission-trust');
assert.equal(element('trust').disabled,true);
context.enterPermissionTrust();context.choosePermissionTrust('key');context.renderPermissionTrustPage();
assert.equal(calls.length,0,'trust entry must never auto-allow, including empty trust groups');
fail=true;await context.respondPermission('deny-hint','reason');
assert.equal(context.pendingPermissions.a.length,2);assert(context.permissionUiStateBySession.a);assert.equal(errors.length,1);
assert.equal(element('permPanel').dataset.interruptionUi,'legacy');
assert.deepEqual(calls[0],['respond_permission',{sessionId:'a',id:'p1',choice:'deny-hint',trustKey:null,trustKeys:null,hint:'reason'}]);
fail=false;await context.respondPermission('allow');
assert.equal(context.pendingPermissions.a.length,1);assert.equal(context.pendingPermissions.a[0].request_id,'p2');
assert.equal(context.pendingPermissions.a[0].needsRecheck,true);assert.equal(context.pendingPermissions.b.length,1);
assert.equal(context.currentPermissionId,null);assert(events.some(e=>e.type==='rozsa:interruption-close'));
context.collectQuestionAnswer=()=> 'custom';
fail=true;await context.submitUserQuestion();
assert.equal(context.pendingUserQuestions.a.length,1);assert.equal(element('questionPanelSubmit').disabled,false);
fail=false;await context.submitUserQuestion();
assert.equal(context.pendingUserQuestions.a,undefined);assert.equal(context.pendingUserQuestions.b.length,1);
assert.deepEqual(calls.at(-1),['respond_user_question',{sessionId:'a',id:'q1',answers:{Choice:'custom'}}]);
assert.equal(context.currentQuestionId,null);
context.pendingUserQuestions.a=[{sessionId:'a',requestId:'q2',questions:[{}]}];
context.activeSessionId='a';context.questionDisplayInFlight=false;
let visible=false,focusVisible=false;
element('questionPanel').classList.add=()=>{visible=true;};
element('questionPanel').querySelector=()=>({focus(){focusVisible=visible;}});
context.renderQuestionPage=()=>{};
await context.displayQuestionPanelIfNeeded();
assert(focusVisible,'question focus must be assigned after the panel becomes visible');
"#,
    );
}
