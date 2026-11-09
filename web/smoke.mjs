import {section} from './test-ui.mjs';
import {chromium} from 'playwright';
import assert from 'node:assert/strict';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',headless:true,args:['--no-sandbox']});
try {
const context=await browser.newContext({viewport:{width:390,height:844},deviceScaleFactor:1,isMobile:true,hasTouch:true,acceptDownloads:true});
const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
await page.goto(process.env.PWA_URL||'http://127.0.0.1:8080');
await page.getByRole('status').filter({hasText:'Ready'}).waitFor();
await section(page,'operations');await page.getByRole('button',{name:'Generate paths',exact:true}).click();
await page.getByRole('status').filter({hasText:'Paths generated'}).waitFor();
assert.match(await page.locator('#stats').textContent(),/128 moves/);
await page.locator('.analysis-actions summary').click();
await page.getByRole('button',{name:'Simulate stock',exact:true}).click();
await page.getByRole('status').filter({hasText:'simulation complete'}).waitFor();
assert.match(await page.locator('#result').textContent(),/Removed:/);
// Native version-one projects migrate through the same shared loader.
await section(page,'project');
page.once('dialog',d=>d.accept());
await page.locator('#open').setInputFiles(new URL('../examples/tray.cam.json',import.meta.url).pathname);
await page.getByRole('status').filter({hasText:'Project opened'}).waitFor();
const save=page.waitForEvent('download');await page.locator('#save').click();assert.match((await save).suggestedFilename(),/\.cam\.json$/);
await section(page,'operations');const download=page.waitForEvent('download');await page.locator('#export').click();assert.match((await download).suggestedFilename(),/\.nc$/);
await section(page,'operations');
// Failed validation must not allow an export or leave a stale preview.
await page.getByLabel('depth · mm',{exact:true}).fill('-1');await page.getByLabel('depth · mm',{exact:true}).dispatchEvent('change');
await section(page,'operations');await page.locator('#export').click();await page.locator('#status.error').waitFor();
assert.equal(await page.locator('#stats').textContent(),'Changed · generate to refresh');
await section(page,'operations');await page.getByLabel('depth · mm',{exact:true}).fill('3');await page.getByLabel('depth · mm',{exact:true}).dispatchEvent('change');
assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
await page.screenshot({path:'/tmp/pwa-phone.png',fullPage:true});
await page.waitForFunction(()=>document.getElementById('offline').textContent.includes('installed'));
await context.setOffline(true);await page.reload();await page.getByRole('status').filter({hasText:'Ready'}).waitFor();await section(page,'operations');await page.locator('#generate').click();await page.getByRole('status').filter({hasText:'Paths generated'}).waitFor();
await context.setOffline(false);await page.setViewportSize({width:1024,height:768});await page.screenshot({path:'/tmp/pwa-tablet.png',fullPage:true});
// Mesh import runs in WASM, including unit handling and fitted stock.
await section(page,'model');
page.once('dialog',d=>d.accept());
await page.locator('#model').setInputFiles(new URL('../examples/models/mount.stl',import.meta.url).pathname);
await page.getByRole('status').filter({hasText:'Model imported'}).waitFor();
assert.ok(await page.getByLabel('Stock X · mm',{exact:true}).inputValue());
assert.deepEqual(errors,[]);console.log('PASS: phone layout, shared CAM generation, stock simulation, export, invalid export rejection, offline reload and generation, tablet layout');
await context.close();
}finally{await browser.close()}
