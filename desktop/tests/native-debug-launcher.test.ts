// Exercise a copied Forge project so the user's active development window is untouched.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {join,basename} from 'node:path';
import {writeFile,mkdir,cp,symlink} from 'node:fs/promises';
import {createEnvironment,attachPage,projectRoot} from './performance/harness.js';
const env=await createEnvironment('debug-launcher');
const fixture=join(env.base,'project');await mkdir(fixture);
await cp(join(projectRoot,'contracts'),join(fixture,'contracts'),{recursive:true});
await cp(join(projectRoot,'desktop'),join(fixture,'desktop'),{recursive:true,filter:path=>!['node_modules','.vite','out','dist'].includes(basename(path))});
await cp(join(projectRoot,'devtools'),join(fixture,'devtools'),{recursive:true});
await symlink(join(projectRoot,'desktop/node_modules'),join(fixture,'desktop/node_modules'),process.platform==='win32'?'junction':'dir');
await symlink(join(projectRoot,'backend'),join(fixture,'backend'),process.platform==='win32'?'junction':'dir');
const child=spawn('bash',[join(fixture,'devtools/start-desktop.sh'),'--user-data-dir',env.profile,'--remote-debugging-port=0'],{cwd:env.base,env:{...process.env,ROZSA_CONFIG_DIR:env.config,ROZSA_PROJECT_CONFIG_DIR:env.projectConfig,ROZSA_PERF_TRACE:'0'},detached:true,stdio:['ignore','pipe','pipe']});
let logs='',page;child.stdout.on('data',chunk=>logs+=chunk);child.stderr.on('data',chunk=>logs+=chunk);
const closed=new Promise(resolve=>child.once('close',resolve));
try {
 const deadline=Date.now()+30000;let url;
 while(Date.now()<deadline){url=/DevTools listening on (ws:\/\/\S+)/.exec(logs)?.[1];if(url)break;if(child.exitCode!==null)throw new Error(logs);await new Promise(resolve=>setTimeout(resolve,100));}
 assert.ok(url,logs);page=await attachPage(url);
 await page.waitFor("document.querySelectorAll('#sessionList .session-item').length===3",'three sample sessions');
 await page.waitFor("document.querySelector('#sessionList').inert===false && !document.querySelector('#sessionList').hasAttribute('aria-busy')",'session selection ready');
 await page.evaluate("window.RozsaUIReady");
 await page.evaluate("[...document.querySelectorAll('#sessionList .session-item')].find(row=>row.textContent.includes('Mermaid')).click()");
 await page.waitFor("document.querySelectorAll('.md-mermaid svg.pele').length===2",'sample diagrams rendered');
 console.log(await page.evaluate("[...document.querySelectorAll('#sessionList .session-item')].map(row=>row.textContent.trim())"));
 console.log('Real launcher and two Pele diagrams passed',env.base);
}catch(error){
 console.log(await page?.evaluate("({text:document.body.innerText,diagrams:document.querySelectorAll('.md-mermaid svg').length})"));throw error;
}finally{
 page?.socket.close();process.kill(-child.pid,'SIGTERM');await closed;await writeFile(join(env.base,'launcher.log'),logs);
 const backend=/Backend started: (\d+)/.exec(logs)?.[1];
 if(backend){let alive=true;try{process.kill(Number(backend),0)}catch(error){if(error.code==='ESRCH')alive=false;else throw error}assert.equal(alive,false,'owned Go backend must exit');}
}
