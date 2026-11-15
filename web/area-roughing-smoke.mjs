import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {ready,importModel,ribbon,section,saveOperation,state,closeDialogs,generate} from './test-ui.mjs';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try{
 const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[];page.on('pageerror',e=>errors.push(e.message));await page.goto(process.env.PWA_URL);await ready(page);
 await importModel(page,new URL('../examples/models/mount.stl',import.meta.url).pathname);await section(page,'setup');await page.locator('#fit-top').fill('2');await page.locator('#apply-stock').click();await state(page);await closeDialogs(page);await page.locator('[data-view=top]').click();
 await ribbon(page,'2.5D milling','Area roughing');await page.getByRole('button',{name:'Pick on model',exact:true}).click();
 const point=await page.locator('#canvas').evaluate(canvas=>{const [x,y]=canvas.camViewport.projectWorld([6,6,-2]),r=canvas.getBoundingClientRect();return{x:x+r.left,y:y+r.top}});
 await page.mouse.move(point.x,point.y);await page.waitForFunction(()=>document.getElementById('canvas').dataset.hoverSelection==='true');await page.mouse.click(point.x,point.y);await page.locator('#operation-dialog').waitFor({state:'visible'});
 await page.getByRole('tab',{name:'Passes',exact:true}).click();await page.locator('#operation-dialog').getByLabel('Step down · mm',{exact:true}).fill('0.5');await page.locator('#operation-dialog').getByLabel('Axial stock to leave · mm',{exact:true}).fill('0.2');await saveOperation(page);await generate(page);
 const op=(await state(page)).operations.at(-1);assert.equal(op.strategy,'area_rough');assert.ok(op.settings.islands.length>=5);assert.ok(op.model_link);assert.equal(op.settings.step_down,.5);
 const icons=await page.evaluate(async()=>new DOMParser().parseFromString(await (await fetch('./assets/commands.svg')).text(),'image/svg+xml').querySelector('#slot').innerHTML);assert.ok(icons.includes('<path')&&!icons.includes('<use'),'Slot has an independent through-slot illustration');assert.deepEqual(errors,[]);
 console.log('PASS: Area roughing in Roughing group, horizontal face hover/pick, automatic islands, stepdown/allowance settings, linked generation and dedicated slot illustration');
}finally{await browser.close()}
