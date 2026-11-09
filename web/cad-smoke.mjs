import {section} from './test-ui.mjs';
import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox']});
try{
 const context=await browser.newContext({viewport:{width:390,height:844},isMobile:true,hasTouch:true});
 const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(process.env.PWA_URL||'http://127.0.0.1:8081');await page.locator('#status').filter({hasText:'Ready'}).waitFor();await section(page,'import');
 async function importFile(path){await section(page,'model');page.once('dialog',d=>d.accept());await page.locator('#model').setInputFiles(new URL(path,import.meta.url).pathname);await page.locator('#status').filter({hasText:'Model imported'}).waitFor({timeout:120000});return JSON.parse(await page.locator('#json').inputValue())}
 function bounds(p){const points=p.part.geometry.mesh.vertices;return [0,1,2].map(a=>Math.max(...points.map(v=>v[a]))-Math.min(...points.map(v=>v[a])))}
 for(const [file,expected,format] of [['mount.step',[80,50,12],'step'],['mount.iges',[80,50,12],'iges'],['inch-box.step',[25.4,50.8,12.7],'step']]){
  const p=await importFile(`../examples/models/${file}`);
  assert.equal(p.part.geometry.format,format);assert.equal(p.part.units,'millimetres');assert.ok(p.part.geometry.mesh.triangles.length>0);assert.ok(p.part.geometry.cad_source.length>100);
  bounds(p).forEach((v,i)=>assert.ok(Math.abs(v-expected[i])<.02,`${file}: axis ${i}: ${v} vs ${expected[i]}`));
  // Browser-imported project survives the shared JSON loader unchanged.
  await section(page,'project');page.once('dialog',d=>d.accept());await page.locator('#open').setInputFiles({name:'roundtrip.cam.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(p))});await page.locator('#status').filter({hasText:'Project opened'}).waitFor();assert.deepEqual(JSON.parse(await page.locator('#json').inputValue()),p);
 }
 await page.waitForFunction(()=>document.getElementById('offline').textContent.includes('installed'));await context.setOffline(true);
 const p=await importFile('../tests/fixtures/features-blind-hole.step');assert.ok(p.part.geometry.mesh.triangles.length>0);
 const before=await page.locator('#json').inputValue();await section(page,'import');page.once('dialog',d=>d.accept());await page.locator('#model').setInputFiles({name:'broken.step',mimeType:'application/octet-stream',buffer:Buffer.from('ISO-10303-21; invalid; END-ISO-10303-21;')});await page.locator('#status.error').waitFor({timeout:120000});assert.equal(await page.locator('#json').inputValue(),before);
 assert.deepEqual(errors,[]);console.log('PASS: STEP, IGES, inch-to-mm conversion, native-format roundtrip, offline CAD kernel, complex fixture, failed import preserves workspace');await context.close();
 const full=await browser.newContext();await full.addInitScript(()=>{Storage.prototype.setItem=function(){throw new DOMException('Full','QuotaExceededError')}});
 const q=await full.newPage();await q.goto(process.env.PWA_URL||'http://127.0.0.1:8081');await q.locator('#status').filter({hasText:'Ready'}).waitFor();await section(q,'import');q.once('dialog',d=>d.accept());await q.locator('#model').setInputFiles(new URL('../examples/models/mount.step',import.meta.url).pathname);await q.locator('#status.error').filter({hasText:'Autosave is unavailable'}).waitFor({timeout:120000});await section(q,'project');const download=q.waitForEvent('download');await q.locator('#save').click();assert.match((await download).suggestedFilename(),/\.cam\.json$/);await full.close();
}finally{await browser.close()}
