import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {section} from './test-ui.mjs';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox']});
try{
for(const [width,height] of [[360,800],[390,844],[641,900],[768,1024],[1024,768],[1440,900]]){
 const context=await browser.newContext({viewport:{width,height}});const page=await context.newPage();await page.goto(process.env.PWA_URL||'http://127.0.0.1:8081');await page.locator('#status').filter({hasText:'Ready'}).waitFor();
 await page.locator('#resume-workspace').click();
 const file=page.locator('.file-menu > summary');assert.ok((await file.boundingBox()).x<80);await file.click();assert.equal(await page.locator('#save').isVisible(),true);await page.keyboard.press('Escape');assert.equal(await page.locator('#save').isVisible(),false);
 assert.equal(await page.locator('.pane-nav').isVisible(),true);
 assert.equal(await page.locator('[data-panel=setup]').isVisible(),false);
 await section(page,'setup');assert.equal(await page.getByLabel('Stock X · mm',{exact:true}).isVisible(),true);assert.equal(await page.locator('#fit-stock').isVisible(),true);
 assert.equal(await page.locator('.viewer').isVisible(),true);
 await section(page,'operations');await page.getByRole('button',{name:'+ Add operation',exact:true}).click();await page.waitForFunction(()=>document.getElementById('count').textContent==='3');assert.equal(await page.getByLabel('depth · mm',{exact:true}).isVisible(),true);
 assert.equal(await page.locator('#recognize').isVisible(),true);assert.equal(await page.locator('#export').isVisible(),true);
 await page.locator('#generate').click();await page.locator('#status').filter({hasText:'Paths generated'}).waitFor();
 assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`Overflow at ${width}`);
 await page.screenshot({path:`/tmp/cam-layout-${width}.png`,fullPage:true});
 await page.setViewportSize({width:height,height:width});assert.equal(await page.getByLabel('depth · mm',{exact:true}).isVisible(),true);
 await section(page,'import');page.once('dialog',d=>d.accept());const chooser=page.waitForEvent('filechooser');await page.getByText('Choose model',{exact:true}).click();await (await chooser).setFiles(new URL('../examples/models/mount.stl',import.meta.url).pathname);await page.locator('#status').filter({hasText:'Model imported'}).waitFor();await page.getByRole('button',{name:'Continue to Setup',exact:true}).click();assert.equal(await page.locator('#fit-stock').isVisible(),true);
 await context.close();
}
console.log('PASS: focused phone panels, tablet preview + inspector, desktop controls, operation editing, rotation and no horizontal overflow');
}finally{await browser.close()}
