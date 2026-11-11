import {chromium} from 'playwright';
import assert from 'node:assert/strict';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try{
 for(const width of [390,768,1440]){
  const context=await browser.newContext({viewport:{width,height:900},hasTouch:true}),page=await context.newPage();
  await page.goto(process.env.PWA_URL||'http://127.0.0.1:8097');await page.locator('#status').filter({hasText:'Ready'}).waitFor();
  assert.equal(await page.locator('#home').isVisible(),true);assert.equal(await page.locator('dialog[open]').count(),0);assert.equal(await page.locator('#resume-workspace').isVisible(),false);
  assert.doesNotMatch(await page.locator('#title').textContent(),/tray/i);assert.equal(await page.evaluate(()=>localStorage.getItem('oc-project')),null);
  await page.locator('#project-properties').click();assert.equal(await page.locator('dialog[open]').count(),0);
  await page.locator('[data-process=milling]').click();await page.locator('#status').filter({hasText:'Milling workspace opened'}).waitFor();
  const p=await page.evaluate(()=>JSON.parse(localStorage.getItem('oc-project')));assert.deepEqual(p.operations,[]);assert.equal(p.part??null,null);assert.doesNotMatch(p.name,/tray/i);assert.equal(await page.locator('#project-dialog').isVisible(),false);
  await page.reload();await page.locator('#status').filter({hasText:'Ready'}).waitFor();assert.equal(await page.locator('#home').isVisible(),true);assert.equal(await page.locator('#resume-workspace').isVisible(),true);assert.equal(await page.locator('dialog[open]').count(),0);
  await page.locator('#resume-workspace').click();assert.equal(await page.locator('.viewer').isVisible(),true);assert.equal(await page.locator('#project-dialog').isVisible(),false);
  await context.close();console.log(`PASS: ${width}px empty first launch, process chooser, passive project title and explicit saved-workspace resume`);
 }
}finally{await browser.close()}
