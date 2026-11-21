import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {ready,importModel,ribbon,section,saveOperation,state,closeDialogs,generate,editOperation} from './test-ui.mjs';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try{
 const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[];page.on('pageerror',e=>errors.push(e.message));await page.addInitScript(()=>{const Original=Worker;window.Worker=class extends Original{constructor(...args){super(...args);this.addEventListener('message',({data})=>{if(data.value?.moves)window.auditMoves=data.value.moves})}}});await page.goto(process.env.PWA_URL);await ready(page);
 await importModel(page,new URL('../examples/models/mount.stl',import.meta.url).pathname);await section(page,'setup');await page.locator('#fit-top').fill('2');await page.locator('#apply-stock').click();await state(page);await closeDialogs(page);await page.locator('[data-view=top]').click();
 await ribbon(page,'2.5D milling','2D pocket clearing');await page.getByRole('button',{name:'Pick on model',exact:true}).click();
 const point=await page.locator('#canvas').evaluate(canvas=>{const [x,y]=canvas.camViewport.projectWorld([6,6,-2]),r=canvas.getBoundingClientRect();return{x:x+r.left,y:y+r.top}});
 await page.mouse.move(point.x,point.y);await page.waitForFunction(()=>document.getElementById('canvas').dataset.hoverSelection==='true');await page.mouse.click(point.x,point.y);await page.locator('#operation-dialog').waitFor({state:'visible'});
 await page.getByRole('tab',{name:'Passes',exact:true}).click();await page.locator('#operation-dialog').getByLabel('Step down · mm',{exact:true}).fill('0.5');await page.locator('#operation-dialog').getByLabel('Axial stock to leave · mm',{exact:true}).fill('0.2');await saveOperation(page);await generate(page);
 const op=(await state(page)).operations.at(-1);assert.equal(op.strategy,'pocket');assert.equal(op.settings.islands.length,0);assert.ok(op.model_link);assert.equal(op.settings.step_down,.5);
 const stock=(await state(page)).stock;
 const extent=await page.evaluate(()=>{const cuts=window.auditMoves.filter(m=>!m.rapid&&m.from[2]===m.to[2]);const points=cuts.flatMap(m=>[m.from,m.to]);return{minX:Math.min(...points.map(p=>p[0])),maxX:Math.max(...points.map(p=>p[0])),minY:Math.min(...points.map(p=>p[1])),maxY:Math.max(...points.map(p=>p[1]))}});
 assert.ok(extent.minX>=5&&extent.maxX<=stock.width-5&&extent.minY>=5&&extent.maxY<=stock.height-5,'Pocket picking must retain its selected floor boundary');
 await editOperation(page);await page.getByRole('tab',{name:'Operation',exact:true}).click();assert.equal(await page.getByLabel('Pocket shape',{exact:true}).inputValue(),'pocket');assert.match(await page.getByLabel('Pocket shape',{exact:true}).textContent(),/Selected pocket region/);await closeDialogs(page);
 await page.reload();await ready(page);assert.equal((await state(page)).operations.at(-1).settings.clearing_boundary,'selected');await closeDialogs(page);
 const icons=await page.evaluate(async()=>new DOMParser().parseFromString(await (await fetch('./assets/commands.svg')).text(),'image/svg+xml').querySelector('#slot').innerHTML);assert.ok(icons.includes('<path')&&!icons.includes('<use'),'Slot has an independent through-slot illustration');assert.deepEqual(errors,[]);
 await ribbon(page,'2.5D milling');for(const name of ['Face milling','2D pocket clearing','Outside clearing','Slot milling','2D contour','Planar finishing','Bore finishing','Contour engraving','Chamfer milling'])assert.equal(await page.locator('#ribbon-commands').getByRole('button',{name,exact:true}).count(),1);
 console.log('PASS: 2D offset clearing and consistent machining names, horizontal face hover/pick, no false islands over holes, stepdown/allowance settings, linked generation and dedicated slot illustration');
}finally{await browser.close()}
