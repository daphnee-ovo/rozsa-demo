import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { attachPage, createEnvironment, startElectron } from "./performance/harness.js";

// Real Chromium layout and pointer hit testing are required; jsdom has neither.
const environment = await createEnvironment("native-layout");
await writeFile(join(environment.sessions, "diagram.jsonl"), [
  { type: "session", version: 3, id: "layout", timestamp: "t", cwd: "/synthetic" },
  { type: "message", id: "diagram", parentId: null, timestamp: "t", message: {
    role: "user", content: "```mermaid\nflowchart LR\n  A[Request] --> B[Streamdown] --> C[Pele]\n```",
  } },
].map(value => JSON.stringify(value)).join("\n") + "\n");
const app = await startElectron(environment);
let page: Awaited<ReturnType<typeof attachPage>> | undefined;
try {
  page = await attachPage(await app.devToolsURL());
  await page.waitFor("!!window.RozsaConversation && document.querySelectorAll('#sessionList .session-item').length===1", "UI ready");
  await page.evaluate(`window.RozsaUIHost.port.request('switch_session',{path:${JSON.stringify(join(environment.sessions, "diagram.jsonl"))}})`);
  await page.waitFor("!!document.querySelector('.md-mermaid svg.pele')", "diagram mounted");
  const screenshot = async (name: string) => {
    const { data } = await page!.send("Page.captureScreenshot", { format: "png" });
    await writeFile(join(environment.base, name + ".png"), Buffer.from(data, "base64"));
  };
  const click = async (selector: string) => {
    if (selector !== ".sidebar-toggle-button") await page!.evaluate(`document.querySelector(${JSON.stringify(selector)}).scrollIntoView({block:'center'})`);
    const point = await page!.evaluate(`(()=>{const r=document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()`);
    await page!.send("Input.dispatchMouseEvent", { type: "mouseMoved", ...point });
    await page!.evaluate("new Promise(resolve=>setTimeout(resolve,150))");
    const hit = await page!.evaluate(`(()=>{const e=document.elementFromPoint(${point.x},${point.y});return {matches:!!e?.closest(${JSON.stringify(selector)}),html:e?.outerHTML.slice(0,300),point:${JSON.stringify(point)},scale:getComputedStyle(document.body).zoom}})()`);
    assert.ok(hit.matches, `Control is covered: ${selector}: ${JSON.stringify(hit)}`);
    await page!.send("Input.dispatchMouseEvent", { type: "mousePressed", ...point, button: "left", clickCount: 1 });
    await page!.send("Input.dispatchMouseEvent", { type: "mouseReleased", ...point, button: "left", clickCount: 1 });
  };
  for (const scale of [0.5, 1, 1.5, 2]) {
    assert.equal(await page.evaluate(`saveSetting('appearance_font_size','${14 * scale}')`), true);
    const bounds = await page.evaluate(`(()=>{const r=document.querySelector('#mainContentScene').getBoundingClientRect();return {right:r.right,bottom:r.bottom,width:innerWidth,height:innerHeight}})()`);
    assert.ok(Math.abs(bounds.right - bounds.width) < 1 && Math.abs(bounds.bottom - bounds.height) < 1, JSON.stringify({ scale, bounds }));
    await click(".sidebar-toggle-button");
    await page.waitFor("document.querySelector('[data-od-id=app-body]').classList.contains('sidebar-collapsed')", "collapse by native click");
    assert.ok(await page.evaluate("document.querySelector('#mainContentScene').getBoundingClientRect().left===0"));
    await click(".sidebar-toggle-button");
    await page.waitFor("!document.querySelector('[data-od-id=app-body]').classList.contains('sidebar-collapsed')", "reopen by native click");
  }
  assert.equal(await page.evaluate("saveSetting('appearance_font_size','14')"), true);
  await page.waitFor("document.querySelectorAll('.md-mermaid-actions .pele-zoom button svg.lucide').length===3", "Lucide native zoom controls");
  assert.ok(await page.evaluate(`(()=>{const buttons=[document.querySelector('[aria-label="Show source"]'),document.querySelector('[aria-label="Zoom in"]')];return ['width','height','border','borderRadius','backgroundColor'].every(property=>getComputedStyle(buttons[0])[property]===getComputedStyle(buttons[1])[property])})()`));
  assert.ok(await page.evaluate(`(()=>{const svg=document.querySelector('.md-mermaid svg.pele'),before=svg.getAttribute('viewBox');svg.dispatchEvent(new MouseEvent('dblclick',{bubbles:true,cancelable:true}));return svg.getAttribute('viewBox')===before})()`), 'double-click must not zoom');
  await click('[aria-label="Show source"]');
  await page.waitFor("!!document.querySelector('.md-mermaid-source')", "source view");
  assert.ok(await page.evaluate("!document.querySelector('.md-mermaid svg') && document.querySelector('.md-mermaid-return').nextElementSibling.matches('.md-copy')"));
  assert.ok(await page.evaluate(`(()=>{const buttons=[...document.querySelectorAll('.md-mermaid-source .md-code-head button')];return ['width','height','border','borderRadius','backgroundColor','color'].every(property=>getComputedStyle(buttons[0])[property]===getComputedStyle(buttons[1])[property])})()`));
  await screenshot("source");
  await click('[aria-label="Show diagram"]');
  await page.waitFor("!!document.querySelector('.md-mermaid svg.pele')", "diagram restored");
  await page.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 600, y: 200 });
  const toolbar = await page.evaluate(`(()=>{const w=document.querySelector('.md-mermaid-wrap').getBoundingClientRect(),t=document.querySelector('.md-mermaid-actions').getBoundingClientRect();return {right:w.right-t.right,top:t.top-w.top}})()`);
  // The user message paper is rotated by .6deg.
  assert.ok(Math.abs(toolbar.right - 8) < 2 && Math.abs(toolbar.top - 8) < 2, JSON.stringify(toolbar));
  await click('[aria-label="Enter fullscreen"]');
  await page.waitFor("!!document.querySelector('body > .md-mermaid-fullscreen svg.pele')", "in-app diagram viewer");
  assert.equal(await page.evaluate("document.fullscreenElement"), null);
  await page.evaluate(`document.addEventListener('click',event=>{const link=event.target.closest('a[download="diagram.png"]');if(link){event.preventDefault();window.__savedDiagram=link.href;}},true)`);
  await click('[aria-label="Save image"]');
  await page.waitFor("window.__savedDiagram?.startsWith('data:image/png;base64,')", "PNG image export");
  const png = await page.evaluate("window.__savedDiagram");
  await writeFile(join(environment.base, "diagram.png"), Buffer.from(png.split(",")[1], "base64"));
  const expanded = await page.evaluate("document.querySelector('.md-mermaid-fullscreen svg.pele').getAttribute('viewBox')");
  await click('.md-mermaid-fullscreen [aria-label="Zoom in"]');
  assert.notEqual(await page.evaluate("document.querySelector('.md-mermaid-fullscreen svg.pele').getAttribute('viewBox')"), expanded);
  await screenshot("diagram-viewer");
  await click('[aria-label="Exit fullscreen"]');
  await page.waitFor("!document.querySelector('.md-mermaid-fullscreen') && !!document.querySelector('.md-mermaid svg.pele')", "exit diagram viewer");
  // Exercise the real native appearance setting, including its CSS projection.
  assert.equal(await page.evaluate("saveSetting('appearance_translucent_sidebar','true')"), true);
  await page.waitFor("document.documentElement.dataset.themeTranslucentSidebar==='true'", "native material enabled");
  let lightTintAlpha;
  for (const mode of ["light", "dark"]) {
    assert.equal(await page.evaluate(`saveSetting('appearance_theme_mode','${mode}')`), true);
    assert.equal(await page.evaluate("saveSetting('appearance_translucent_sidebar','false')"), true);
    const mainBackground = await page.evaluate("getComputedStyle(document.querySelector('#mainContentScene')).backgroundColor");
    const opaque = await page.evaluate(`(()=>{
      const sidebar=getComputedStyle(document.querySelector('[data-od-id=sidebar]')),main=getComputedStyle(document.querySelector('#mainContentScene'));
      const canvas=document.createElement('canvas');canvas.width=canvas.height=1;const context=canvas.getContext('2d');context.fillStyle=sidebar.backgroundColor;context.fillRect(0,0,1,1);
      return {sidebarShadow:sidebar.boxShadow,mainShadow:main.boxShadow,sidebarZ:Number(sidebar.zIndex),mainZ:Number(main.zIndex),alpha:context.getImageData(0,0,1,1).data[3]};
    })()`);
    assert.equal(opaque.sidebarShadow, 'none', `${mode} opaque sidebar must use the shared layer rules`);
    assert.ok(opaque.sidebarZ < opaque.mainZ, 'opaque main panel must be above the sidebar');
    assert.equal(opaque.alpha, 255, 'disabled translucency must remain fully opaque');
    if (mode === 'dark') assert.equal(opaque.mainShadow, 'none');
    else assert.notEqual(opaque.mainShadow, 'none');
    await screenshot(mode + '-opaque-sidebar');
    assert.equal(await page.evaluate("saveSetting('appearance_translucent_sidebar','true')"), true);
    assert.equal(await page.evaluate("getComputedStyle(document.querySelector('#mainContentScene')).backgroundColor"), mainBackground, 'sidebar translucency must preserve the main theme');
    assert.ok(await page.evaluate(`(()=>{const s=document.querySelector('[data-od-id=sidebar]'),r=s.getBoundingClientRect(),body=getComputedStyle(document.body);return r.top===0&&Math.abs(r.bottom-innerHeight)<1&&getComputedStyle(document.querySelector('[data-od-id=app-body]')).backgroundColor==='rgba(0, 0, 0, 0)'&&body.backgroundImage==='none'&&body.backgroundColor==='rgba(0, 0, 0, 0)'})()`), 'no opaque gradient may crop the native sidebar material');
    const layers = await page.evaluate(`(()=>{
      const canvas=document.createElement('canvas');canvas.width=canvas.height=1;const context=canvas.getContext('2d');
      const sidebar=getComputedStyle(document.querySelector('[data-od-id=sidebar]')),main=getComputedStyle(document.querySelector('#mainContentScene'));
      context.fillStyle=sidebar.backgroundColor;context.fillRect(0,0,1,1);
      return {sidebarShadow:sidebar.boxShadow,mainShadow:main.boxShadow,sidebarZ:Number(sidebar.zIndex),mainZ:Number(main.zIndex),alpha:context.getImageData(0,0,1,1).data[3]/255};
    })()`);
    assert.equal(layers.sidebarShadow, 'none');
    assert.ok(layers.sidebarZ < layers.mainZ, 'main panel must be above the sidebar');
    if (mode === 'light') { lightTintAlpha = layers.alpha; assert.notEqual(layers.mainShadow, 'none'); }
    else { assert.equal(layers.mainShadow, 'none'); assert.ok(layers.alpha < lightTintAlpha && layers.alpha > 0.5, 'dark material must be more transparent while retaining theme tint'); }
    await screenshot(mode + "-sidebar");
    for (const translucent of [true, false]) {
      assert.equal(await page.evaluate(`saveSetting('appearance_translucent_sidebar','${translucent}')`), true);
      const activeSession = await page.evaluate("document.querySelector('#sessionList .session-item.active')?.textContent");
      await page.evaluate("toggleSettings()");
      await page.waitFor("document.querySelector('#settingsPanel.visible .settings-tab') && document.body.classList.contains('settings-visible')", "settings scene");
      assert.ok(await page.evaluate(`(()=>{const app=document.querySelector('[data-od-id=app-body]');return getComputedStyle(app).contentVisibility==='hidden' && app.inert})()`), 'settings must replace the main scene, including descendants with explicit visibility');
      assert.equal(await page.evaluate("getComputedStyle(document.querySelector('.settings-dialog'),'::before').webkitAppRegion"), 'drag', 'settings titlebar must drag the native window');
      assert.equal(await page.evaluate("getComputedStyle(document.querySelector('.settings-tab')).webkitAppRegion"), 'no-drag', 'settings navigation must remain clickable');
      await screenshot(`${mode}-settings-${translucent ? 'translucent' : 'opaque'}`);
      await click('.settings-back');
      await page.waitFor("!!document.querySelector('.md-mermaid svg.pele')", 'diagram restored after settings');
      assert.ok(await page.evaluate(`(()=>{const app=document.querySelector('[data-od-id=app-body]');return app.getClientRects().length>0 && !app.inert})()`), 'returning must restore the main scene');
      assert.equal(await page.evaluate("document.querySelector('#sessionList .session-item.active')?.textContent"), activeSession, 'returning must preserve the selected session');
    }
  }
  console.log(`Native layout, sidebar and Mermaid checks passed; screenshots: ${environment.base}`);
} catch (error) {
  console.error(await page?.evaluate("({text:document.body.innerText})"));
  throw error;
} finally {
  page?.socket.close();
  await app.stop();
}
