import {section,editOperation,saveOperation,closeDialogs,generate,inspect} from './test-ui.mjs';
import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {cpSync,mkdtempSync,readFileSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {spawn} from 'node:child_process';
const root=mkdtempSync(`${tmpdir()}/cam-update-`);cpSync(new URL('../dist/pwa',import.meta.url),root,{recursive:true});
const server=spawn('python3',['-m','http.server','8124','--bind','127.0.0.1','--directory',root],{stdio:'ignore'});
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox']});
try{
for(let i=0;i<50;i++){try{await fetch('http://127.0.0.1:8124');break}catch{await new Promise(r=>setTimeout(r,100))}}
const context=await browser.newContext();const page=await context.newPage();
await page.goto('http://127.0.0.1:8124');await page.locator('#status').filter({hasText:'Ready'}).waitFor();
async function equalEventually(read,expected){const deadline=Date.now()+5000;while(await read()!==expected&&Date.now()<deadline)await new Promise(r=>setTimeout(r,30));assert.equal(await read(),expected)}
assert.equal(await page.locator('#update').isVisible(),false);
await editOperation(page);await page.getByLabel('depth · mm',{exact:true}).fill('4');await saveOperation(page);
const depth=()=>page.evaluate(()=>JSON.parse(localStorage.getItem('oc-project')).operations[0].depth);
await page.locator('#undo').click();await equalEventually(depth,3);await page.locator('#redo').click();await equalEventually(depth,4);await page.reload();await page.locator('#status').filter({hasText:'Ready'}).waitFor();await equalEventually(depth,4);
await closeDialogs(page);if(!await page.locator('.op').first().isVisible())await page.locator('#browser-toggle').click();await page.locator('.op').first().click({button:'right'});await page.getByRole('menuitem',{name:'Duplicate operation',exact:true}).click();await equalEventually(()=>page.locator('#count').textContent(),'3');await page.locator('.op').nth(1).click({button:'right'});await page.getByRole('menuitem',{name:'Move operation down',exact:true}).click();await page.locator('#undo').click();await page.locator('#undo').click();await equalEventually(()=>page.locator('#count').textContent(),'2');await page.locator('.op').first().click({button:'right'});await page.getByRole('menuitem',{name:'Delete operation',exact:true}).click();await equalEventually(()=>page.locator('#count').textContent(),'1');await page.locator('#undo').click();await equalEventually(()=>page.locator('#count').textContent(),'2');
await page.waitForFunction(()=>document.getElementById('offline').textContent.includes('installed'));
await page.waitForFunction(()=>!!navigator.serviceWorker.controller);
let sw=readFileSync(`${root}/sw.js`,'utf8');const old=await page.locator('#version').textContent();sw=sw.replace(/opensourcecam-[a-f0-9]{12}/,'opensourcecam-test-upgrade');writeFileSync(`${root}/sw.js`,sw);writeFileSync(`${root}/build.js`,"export const BUILD_ID='test-upgrade';");
await page.evaluate(async()=>{const reg=await navigator.serviceWorker.getRegistration();await reg.update()});await page.locator('#update').waitFor({state:'visible'});
// A second download can supersede a worker while the update button is already shown.
await page.evaluate(async()=>{window.firstWaiting=(await navigator.serviceWorker.getRegistration()).waiting});
writeFileSync(`${root}/sw.js`,sw.replace('test-upgrade','test-upgrade-final'));
writeFileSync(`${root}/build.js`,"export const BUILD_ID='test-upgrade-final';");
await page.evaluate(async()=>{await (await navigator.serviceWorker.getRegistration()).update()});
await page.waitForFunction(async()=>{const reg=await navigator.serviceWorker.getRegistration();return window.firstWaiting?.state==='redundant'&&reg.waiting&&reg.waiting!==window.firstWaiting&&reg.waiting.state==='installed'&&!reg.installing});
await equalEventually(depth,4);await page.locator('#update').click();try{await page.waitForFunction(()=>document.getElementById('version')?.textContent==='Build test-upgrade-final')}catch(error){throw Error(JSON.stringify(await page.evaluate(async()=>({version:document.getElementById('version')?.textContent,status:document.getElementById('status')?.textContent,waiting:(await navigator.serviceWorker.getRegistration())?.waiting?.state,caches:await caches.keys()})))+' '+error.message)};await equalEventually(depth,4);assert.notEqual(old,await page.locator('#version').textContent());
await context.setOffline(true);await page.reload();await page.locator('#status').filter({hasText:'Ready'}).waitFor();await equalEventually(depth,4);await generate(page);
// Invalid drafts from older versions remain repairable; export still checks the core.
await section(page,'project');await page.locator('#advanced-editor > summary').click();const invalid=JSON.parse(await page.locator('#json').inputValue());invalid.stock.width=-1;await page.locator('#json').fill(JSON.stringify(invalid));await page.locator('#apply').click();await page.locator('#status.error').waitFor();assert.equal(await page.evaluate(()=>JSON.parse(localStorage.getItem('oc-project')).stock.width),120);
await context.close();
const recovery=await browser.newContext();await recovery.addInitScript(()=>localStorage.setItem('oc-project','broken original draft'));const p=await recovery.newPage();await p.goto('http://127.0.0.1:8124');await p.locator('#raw-backup').waitFor({state:'visible'});assert.equal(await p.evaluate(()=>localStorage.getItem('oc-project')),'broken original draft');await recovery.close();
const full=await browser.newContext();await full.addInitScript(()=>{Storage.prototype.setItem=function(){throw new DOMException('Full','QuotaExceededError')}});const q=await full.newPage();await q.goto('http://127.0.0.1:8124');await q.locator('#status').filter({hasText:'Ready'}).waitFor();await editOperation(q);await q.getByLabel('depth · mm',{exact:true}).fill('4');await saveOperation(q);await q.locator('#saved').filter({hasText:'Not saved'}).waitFor();await full.close();
console.log('PASS: undo/redo, autosave reload, waiting upgrade, version change, preserved edits, offline upgraded CAM, corrupt recovery, quota warning');
}finally{await browser.close();server.kill();rmSync(root,{recursive:true,force:true})}
