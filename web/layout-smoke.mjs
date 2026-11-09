import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {section} from './test-ui.mjs';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox']});
try{
for(const [width,height] of [[360,800],[390,844],[641,900],[768,1024],[1024,768],[1440,900]]){
 const context=await browser.newContext({viewport:{width,height}});const page=await context.newPage();await page.goto(process.env.PWA_URL||'http://127.0.0.1:8081');await page.locator('#status').filter({hasText:'Ready'}).waitFor();
 assert.equal(await page.locator('.viewer').isVisible(),true);
 if(width<=1200){assert.equal(await page.locator('[data-panel=setup]').isVisible(),false);await section(page,'setup');assert.equal(await page.getByLabel('Stock X · mm',{exact:true}).isVisible(),true);assert.equal(await page.locator('.viewer').isVisible(),width>640);await section(page,'operations');await page.getByRole('button',{name:'+ Add operation',exact:true}).click();await page.waitForFunction(()=>document.getElementById('count').textContent==='3');assert.equal(await page.getByLabel('depth · mm',{exact:true}).isVisible(),true);await section(page,'project');assert.equal(await page.locator('#save').isVisible(),true);assert.equal(await page.locator('#export').isVisible(),true);await section(page,'model');}
 else{assert.equal(await page.locator('.pane-nav').isVisible(),false);assert.equal(await page.getByLabel('Stock X · mm',{exact:true}).isVisible(),true)}
 assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`Overflow at ${width}`);
 await page.locator('#generate').click();await page.locator('#status').filter({hasText:'Paths generated'}).waitFor();
 await page.screenshot({path:`/tmp/cam-layout-${width}.png`,fullPage:true});
 // Layout changes preserve the same project and navigation remains usable after rotation.
 await page.setViewportSize({width:height,height:width});await section(page,'operations');assert.equal(await page.getByLabel('depth · mm',{exact:true}).isVisible(),true);
 if(width<=640){await page.setViewportSize({width,height});await section(page,'model');page.once('dialog',d=>d.accept());const chooser=page.waitForEvent('filechooser');await page.getByText('Import CAD / mesh',{exact:true}).click();await (await chooser).setFiles(new URL('../examples/models/mount.stl',import.meta.url).pathname);await page.locator('#status').filter({hasText:'Model imported'}).waitFor()}
 await context.close();
}
console.log('PASS: focused phone panels, tablet preview + inspector, desktop controls, operation editing, rotation and no horizontal overflow');
}finally{await browser.close()}
