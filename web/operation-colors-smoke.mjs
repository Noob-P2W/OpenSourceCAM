import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {ready,workspace,generate,idle,state} from './test-ui.mjs';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try{
 const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[];page.on('pageerror',e=>errors.push(e.message));await page.goto(process.env.PWA_URL);await ready(page);await workspace(page);
 await page.locator('.op').first().click({button:'right'});await page.getByRole('menuitem',{name:'Duplicate operation',exact:true}).click();await idle(page);assert.equal((await state(page)).operations.length,3);await generate(page);await page.locator('.playback-dock > summary').click();
 await page.waitForFunction(()=>document.getElementById('progress').style.backgroundImage.startsWith('linear-gradient'));
 assert.equal(await page.locator('#path-sections').count(),0);assert.equal(await page.locator('.playback-dock input[type=range]').count(),1);
 const gradient=await page.locator('#progress').evaluate(i=>i.style.backgroundImage);assert.match(gradient,/rgb\(31, 119, 180\)/);assert.match(gradient,/rgb\(148, 103, 189\)/);assert.ok(gradient.includes('calc('),'Same-type neighbors retain a visible operation boundary');
 await page.locator('.op').nth(0).click();await page.locator('.op').nth(2).click({modifiers:['Control']});assert.equal(await page.locator('.op[aria-selected=true]').count(),2);await page.locator('#selected-path').check();
 await page.waitForFunction(()=>{const b=document.getElementById('canvas').camViewport.pathBuffers;return b?.length&&b.every(i=>i.operation===0||i.operation===2)});
 await page.locator('#playback-clock').hover();await page.waitForFunction(()=>{const v=document.getElementById('canvas').camViewport;return v.controls.hoveredOperation==null&&v.pathBuffers.find(i=>i.operation===2&&!i.rapid).drawColor.every(c=>c<.8)});
 const before=await page.locator('#canvas').evaluate(c=>c.camViewport.pathBuffers.filter(i=>i.operation===2&&!i.rapid).map(i=>({id:i.three.object.geometry.uuid,color:i.drawColor})));
 await page.locator('.op').nth(2).hover();await page.waitForFunction(()=>{const v=document.getElementById('canvas').camViewport;return v.controls.hoveredOperation===2&&v.pathBuffers.find(i=>i.operation===2&&!i.rapid).drawColor.some(c=>c>.8)});
 const after=await page.locator('#canvas').evaluate(c=>c.camViewport.pathBuffers.filter(i=>i.operation===2&&!i.rapid).map(i=>({id:i.three.object.geometry.uuid,color:i.drawColor})));assert.equal(before[0].id,after[0].id);assert.notDeepEqual(before[0].color,after[0].color);
 await page.locator('.op').nth(0).click();await page.locator('.op').nth(2).click({modifiers:['Shift']});assert.equal(await page.locator('.op[aria-selected=true]').count(),3);
 const target=await page.locator('#canvas').evaluate(c=>{const p=c.camViewport.pathSource,m=p.moves.find(m=>m.operation===1);return (m.start_seconds+m.end_seconds)/2/p.seconds*100});
 await page.locator('#progress').evaluate((input,value)=>{input.value=value;input.dispatchEvent(new Event('input',{bubbles:true}))},target);const progress=Number(await page.locator('#progress').inputValue());assert.ok(progress>0&&progress<100);await page.waitForFunction(()=>document.getElementById('progress').dataset.operation==='1');await page.waitForFunction(()=>{const b=document.getElementById('canvas').camViewport.pathBuffers;return b.filter(i=>i.operation===2).every(i=>i.drawCount===0)&&b.some(i=>i.operation===0&&i.drawCount>0)});
 await page.screenshot({path:'/tmp/cam-operation-colors.png'});
 await page.locator('.op').nth(0).click({button:'right'});await page.getByRole('menuitem',{name:'Disable 3 selected operations',exact:true}).click();await idle(page);assert.ok((await state(page)).operations.every(op=>!op.enabled));assert.equal(await page.locator('.op[aria-selected=true]').count(),3);assert.equal(await page.locator('#progress').evaluate(i=>i.style.backgroundImage),'');await page.locator('#undo').click();await idle(page);assert.ok((await state(page)).operations.every(op=>op.enabled));assert.deepEqual(errors,[]);
 console.log('PASS: type colors, timed operation sections, hover without geometry rebuild, Ctrl/Shift path filtering, timeline seek and atomic batch enable/disable undo');
}finally{await browser.close()}
