// FrameworkTree
// ime_input_test.rs
// ├── input_declares_ime_lifecycle_handlers()
// ├── composition_guards_input_refresh_and_keyboard_shortcuts()
// ├── composition_end_refreshes_autocomplete_after_commit()
// └── shared_composer_dispatches_input_lifecycle_and_submit_once()

#[test]
fn input_declares_ime_lifecycle_handlers() {
    let html = include_str!("../frontend/ui/templates/primitives/input.html");
    let bindings = include_str!("../frontend/ui/scripts/components/composer.js");
    let entry = include_str!("../frontend/runtime/entries/main.js");
    assert!(html.contains("id=\"msgInput\""));
    for (event, handler) in [
        ("input", "handleInput"),
        ("compositionstart", "handleCompositionStart"),
        ("compositionupdate", "handleCompositionUpdate"),
        ("compositionend", "handleCompositionEnd"),
    ] {
        assert!(bindings.contains(&format!("addEventListener(\"{event}\"")));
        assert!(bindings.contains(handler));
        assert!(entry.contains(handler));
    }
}

#[test]
fn composition_guards_input_refresh_and_keyboard_shortcuts() {
    let source = include_str!("../frontend/app.js");
    let handle_input_start = source.find("function handleInput(input, event)").unwrap();
    let handle_input_end = source[handle_input_start..]
        .find("function handleCompositionStart")
        .map(|offset| handle_input_start + offset)
        .unwrap();
    let handle_input = &source[handle_input_start..handle_input_end];

    assert!(handle_input.contains("if (isInputComposing ||"));
    assert!(handle_input.contains("event?.isComposing"));
    assert!(handle_input.contains("insertCompositionText"));
    assert!(handle_input.contains("updateAutocomplete();"));

    let keydown_start = source.find("document.addEventListener('keydown'").unwrap();
    let keydown = &source[keydown_start..];
    assert!(
        keydown.contains("if (isInputComposing || e.isComposing || e.keyCode === 229) return;")
    );
}

#[test]
fn composition_end_refreshes_autocomplete_after_commit() {
    let source = include_str!("../frontend/app.js");
    let start = source.find("function handleCompositionEnd(input)").unwrap();
    let end = source[start..]
        .find("function setInputText")
        .map(|offset| start + offset)
        .unwrap();
    let handler = &source[start..end];

    assert!(handler.contains("isInputComposing = false;"));
    assert!(handler.contains("setTimeout"));
    assert!(handler.contains("updateAbortButton();"));
}

#[test]
fn shared_composer_dispatches_input_lifecycle_and_submit_once() {
    let script = r#"
const assert=require('node:assert/strict');
const fs=require('node:fs');
(async()=>{
 const source=fs.readFileSync('crates/rozsa-gui/frontend/ui/scripts/components/composer.js','utf8');
 const {init,renderSendState}=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
 const input={listeners:{},addEventListener(name,fn){this.listeners[name]=fn;}};
 const composer={...input,listeners:{}};
 const calls=[];
 const api={boundModules:new Set(),byId:id=>id==='msgInput'?input:id==='composer'?composer:null};
 for(const name of ['handleInput','handleCompositionStart','handleCompositionUpdate','handleCompositionEnd','addMessage'])api[name]=(...args)=>calls.push([name,...args]);
 init(api);init(api);
 const event={isComposing:true};
 input.listeners.compositionstart(event);
 input.listeners.compositionupdate(event);
 input.listeners.input(event);
 input.listeners.compositionend(event);
 assert.deepEqual(calls.map(x=>x[0]),['handleCompositionStart','handleCompositionUpdate','handleInput','handleCompositionEnd']);
 assert.equal(calls[2][1],input);assert.equal(calls[2][2],event);
 composer.listeners.click({target:{closest(){return {dataset:{action:'send'}};}}});
 assert.equal(calls.filter(x=>x[0]==='addMessage').length,1);
 const button={dataset:{},setAttribute(name,value){this[name]=value;}};
 for(const action of ['send','stop','queue','steer']){renderSendState(button,action);assert.equal(button.dataset.sendAction,action);assert(button['aria-label']);assert(button.innerHTML.includes('<svg'));}
 assert(fs.readFileSync('crates/rozsa-gui/frontend/ui/templates/composites/composer.html','utf8').includes('id="composer"'));
})().catch(error=>{console.error(error);process.exitCode=1;});
"#;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::process::Command::new("node")
        .args(["-e", script])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
