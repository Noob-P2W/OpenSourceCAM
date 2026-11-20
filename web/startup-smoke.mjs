import {chromium} from 'playwright';
import assert from 'node:assert/strict';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try{
 for(const width of [390,768,1440]){
  const context=await browser.newContext({viewport:{width,height:900},hasTouch:true}),page=await context.newPage();
  await page.goto(process.env.PWA_URL||'http://127.0.0.1:8097');await page.locator('#status').filter({hasText:'Ready'}).waitFor();
  assert.equal(await page.locator('#home').isVisible(),true);assert.equal(await page.locator('dialog[open]').count(),0);assert.equal(await page.locator('#resume-workspace').isVisible(),false);
  assert.doesNotMatch(await page.locator('#title').textContent(),/tray/i);assert.equal(await page.evaluate(async ()=>await (await import('./storage.js')).readSnapshot()),null);
  await page.locator('#project-properties').click();assert.equal(await page.locator('dialog[open]').count(),0);
  await page.locator('[data-process=milling]').click();await page.locator('#status').filter({hasText:'Milling workspace opened'}).waitFor();
  const p=await page.evaluate(async ()=>JSON.parse(await (await import('./storage.js')).readSnapshot()));assert.equal(await page.locator('#demo').count(),0);assert.equal(p.tool_library.tools.length,11);for(const kind of ['flat_end_mill','ball_end_mill','face_mill','drill','chamfer_mill','thread_mill'])assert.ok(p.tool_library.tools.some(t=>t.kind===kind));assert.deepEqual(p.operations,[]);assert.equal(p.part??null,null);assert.doesNotMatch(p.name,/tray/i);assert.equal(await page.locator('#project-dialog').isVisible(),false);
  if(width===1440){
   await page.locator('#import-dialog .dialog-close').click();await page.getByRole('tab',{name:'Libraries',exact:true}).click();await page.locator('#ribbon-commands').getByRole('button',{name:'Tool library',exact:true}).click();await page.locator('#library-examples').click();await page.locator('#status').filter({hasText:'Example tools added'}).waitFor();const added=await page.evaluate(async()=>JSON.parse(await (await import('./storage.js')).readSnapshot()));assert.equal(added.tool_library.tools.length,22);assert.deepEqual(added.tool_library.tools.slice(0,11),p.tool_library.tools);assert.equal(added.active_tool,p.active_tool);await page.locator('#tool-library-dialog .dialog-close').click();await page.locator('#undo').click();await page.waitForFunction(async()=>JSON.parse(await (await import('./storage.js')).readSnapshot()).tool_library.tools.length===11);
  }
  await page.reload();await page.locator('#status').filter({hasText:'Ready'}).waitFor();assert.equal(await page.locator('#home').isVisible(),true);assert.equal(await page.locator('#resume-workspace').isVisible(),true);assert.equal(await page.locator('dialog[open]').count(),0);
  await page.locator('#resume-workspace').click();assert.equal(await page.locator('.viewer').isVisible(),true);assert.equal(await page.locator('#project-dialog').isVisible(),false);
  await context.close();console.log(`PASS: ${width}px empty first launch, process chooser, passive project title and explicit saved-workspace resume`);
 }
}finally{await browser.close()}
