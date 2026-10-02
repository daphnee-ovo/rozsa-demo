// FrameworkTree
// shared_tools_ui_test.rs
// └── shared_tools_preserve_output_diff_lines_and_runtime_state_boundaries()

use std::process::Command;

#[test]
fn shared_tools_preserve_output_diff_lines_and_runtime_state_boundaries() {
    let script = r#"
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const root=process.argv[1];
const read=file=>fs.readFileSync(path.join(root,file),'utf8');
assert.doesNotThrow(()=>new Function(fs.readFileSync(path.join(root,'../../prototype/rozsa-gui.js'),'utf8')),'prototype entry must remain syntactically valid');
const source=read('frontend/ui/scripts/features/tools.js');
const changesCss=read('frontend/ui/styles/features/tools.css').match(/\.change-name \{([^}]+)\}/)[1];
assert(changesCss.includes('padding: 1px 6px;'),'shared change disclosure must retain prototype button padding despite runtime button reset');
for(const selector of read('frontend/styles/components/forms.css').matchAll(/([^{}]+)\{/g)) {
  if(selector[1].includes('.turn-diff-inline')) assert(selector[1].trim().startsWith('[data-tool-ui="legacy"]'),'legacy inline diff styles must not override shared diff geometry');
}
const template=read('frontend/ui/templates/composites/tool-call.html').trim();
const tools=new Function('loadTemplate','templateRoot',source.replace(/^import .*;$/gm,'').replace(/export /g,'')+';return {loadToolTemplate,toolMarkup,codeViewMarkup,diffViewMarkup,evidenceMarkup,evidenceViewMarkup,changesMarkup,textMarkup,messageTextMarkup,escapeToolText};')(
  async()=>({firstElementChild:{outerHTML:template}}),new URL('https://fixture.invalid/'));
const adapter=new Function(read('frontend/runtime/adapters/tools.js').replace(/export /g,'')+';return toolView;')();
const activityAdapter=new Function(read('frontend/runtime/adapters/tools.js').replace(/export /g,'')+';return activityView;')();
(async()=>{
  await tools.loadToolTemplate();
  const body=tools.evidenceMarkup('Output','<script>unsafe</script>\nreal stderr');
  const card=tools.toolMarkup('call"1','Bash','echo real','', 's-success',{expanded:true,bodyHtml:body});
  assert(card.includes('tool-call expanded'));assert(card.includes('call&quot;1'));
  assert(!card.includes('<script>'));assert(card.includes('real stderr'));assert(!card.includes('onclick='));
  assert.throws(()=>tools.toolMarkup('id','tool','','','cancelled'),TypeError);
  const diff=tools.diffViewMarkup('@@ -10,2 +20,2 @@\n same\n-old\n+new');
  assert(diff.includes('>20</span>'));assert(diff.includes('same'));assert(diff.includes('>11</span>'));assert(diff.includes('>21</span>'));
  assert(tools.codeViewMarkup('<tag>\nnext').includes('&lt;tag&gt;'));
  assert.equal(tools.messageTextMarkup('plain\ntext\n\nnext'),tools.textMarkup('plain\ntext\n\nnext'));
  assert(tools.messageTextMarkup('```rust\n<tag>\n```').includes('&lt;tag&gt;'));
  for(const missing of ['# Heading','- List','> Quote','[link](https://example.com)','[link][ref]','| Table |','**bold**','*emphasis*','_emphasis_','<b>html</b>','title\n===','~~~\ncode\n~~~','    indented','`inline`','```\nunclosed']) {
    assert.equal(tools.messageTextMarkup(missing),'<div data-ui-suspended="UI-MARKDOWN-001"></div>');
  }
  assert(tools.messageTextMarkup('visible\n\n# Missing\n\nvisible too').includes('<p>visible too</p>'));
  for(const name of ['Read','Bash']) assert.equal(adapter({name},{output:'actual output',isError:false,details:{}}).disposition,'covered');
  assert.equal(adapter({name:'Edit'},{output:'done',details:{file_deltas:[{patch:'@@ -1 +1 @@\n-a\n+b'}]}}).kind,'diff');
  for(const name of ['Read','Edit','Bash','Subagent','Generic','Write']) {
    assert.equal(adapter({name},{output:'real failure',isError:true}).disposition,'blocked');
    assert.equal(adapter({name},null).disposition,'suspended');
  }
  assert.equal(adapter({name:'AskUserQuestion'},null).disposition,'blocked','pending questions must retain their response path');
  assert.equal(adapter({name:'Bash'},{output:'nonzero',details:{exit_code:1}}).disposition,'blocked');
  assert.equal(adapter({name:'Read'},{output:'truncated',details:{truncated:true}}).disposition,'suspended');
  assert.equal(adapter({name:'Subagent'},{output:'done'}).disposition,'suspended');
  assert.equal(adapter({name:'Read'},{output:''}).disposition,'suspended');
  const activity={captureComplete:true,fileChanges:[{path:'src/<safe>.rs',status:'modified',patch:'@@ -1 +1 @@\n-a\n+b',added:1,deleted:1}],verification:{success:true,command:'cargo test',exitCode:0,durationMs:42}};
  const activityCopy=JSON.stringify(activity),activityView=activityAdapter(activity);
  assert.equal(activityView.disposition,'covered');assert.equal(activityView.verification.duration,'42ms');
  assert(tools.changesMarkup(activityView.files,activityView.verification).includes('src/&lt;safe&gt;.rs'));
  assert(tools.changesMarkup(activityView.files,activityView.verification).includes('data-turn-diff='));
  assert.equal(JSON.stringify(activity),activityCopy);
  for(const missing of [{...activity,captureComplete:false},{...activity,verification:{...activity.verification,success:false}},{...activity,fileChanges:[{...activity.fileChanges[0],status:'deleted'}]}]) assert.equal(activityAdapter(missing).disposition,'blocked');
  const renderStart=appSource=>appSource.slice(appSource.indexOf('function renderTurnActivityCard('),appSource.indexOf('function toggleTurnDiff('));
  const renderActivity=new Function('window','escapeHtml',renderStart(read('frontend/app.js'))+';return renderTurnActivityCard;')({RozsaTools:{activityView:activityAdapter,changesMarkup:tools.changesMarkup}},tools.escapeToolText);
  assert.equal(renderActivity(activity),tools.changesMarkup(activityView.files,activityView.verification));
  const failure=renderActivity({...activity,verification:{...activity.verification,success:false}});
  assert(failure.includes('Verification failed'));assert(failure.includes('data-ui-migration-blocked="UI-EVIDENCE-001"'));
  const app=read('frontend/app.js');
  const start=app.indexOf('function toggleToolCall(el)');
  const end=app.indexOf('function toggleThinking(',start);
  const toggle=new Function('el','activeSessionId','expandedToolCallsBySession',app.slice(start,end)+';toggleToolCall(el);');
  const sessions={};
  const cardState={dataset:{sessionId:'session-a',toolCallId:'tool-a'},expanded:false,classList:{contains(){return cardState.expanded;},toggle(_,value){cardState.expanded=value;}}};
  toggle(cardState,'session-b',sessions);assert.deepEqual(sessions,{'session-a':['tool-a']});
  toggle(cardState,'session-b',sessions);assert.deepEqual(sessions,{'session-a':[]});
})().catch(error=>{console.error(error);process.exitCode=1;});
"#;
    let output = Command::new("node")
        .args(["-e", script, env!("CARGO_MANIFEST_DIR")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
