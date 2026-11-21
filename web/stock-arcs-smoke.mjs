import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {ready,ribbon,saveOperation,state,editOperation,closeDialogs} from './test-ui.mjs';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try {
 const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[];page.on('pageerror',e=>errors.push(e.message));await page.goto(process.env.PWA_URL);await ready(page);
 await editOperation(page);await page.getByRole('tab',{name:'Heights & accuracy',exact:true}).click();await page.getByLabel('Remaining-stock clearance · mm',{exact:true}).fill('0.75');await saveOperation(page);assert.equal((await state(page)).operations[0].settings.stock_clearance,0.75);
 await ribbon(page,'Actions','Stock simulation');assert.equal(await page.locator('#conservative-stock').isChecked(),true);await closeDialogs(page);
 await ribbon(page,'Libraries','Active machine');
 const motion=page.getByLabel('Motion output',{exact:true});
 assert.equal(await motion.locator('option[value=spline]').evaluate(option=>option.disabled),true);
 await motion.selectOption('linear');await motion.blur();
 await page.waitForFunction(async()=>JSON.parse(await (await import('./storage.js')).readSnapshot()).post.motion_output==='linear');
 await page.getByLabel('Postprocessor',{exact:true}).selectOption('linux_cnc');
 await page.getByLabel('Postprocessor',{exact:true}).blur();
 await page.waitForFunction(async()=>JSON.parse(await (await import('./storage.js')).readSnapshot()).post.processor==='linux_cnc');
 await motion.selectOption('spline');await motion.blur();
 await page.waitForFunction(async()=>JSON.parse(await (await import('./storage.js')).readSnapshot()).post.motion_output==='spline');
 await closeDialogs(page);
 const result=await page.evaluate(async()=>{
  const p=JSON.parse(await (await import('./storage.js')).readSnapshot());p.part=null;p.workspace=[];p.operations=[];
  const worker=new Worker('./worker.js',{type:'module'});let id=0;
  const call=(action,data={})=>new Promise((resolve,reject)=>{const n=++id;const listen=({data:d})=>{if(d.id!==n)return;worker.removeEventListener('message',listen);d.error?reject(Error(d.error)):resolve(d.value)};worker.addEventListener('message',listen);worker.postMessage({id:n,action,...data})});
  try {
   await call('session-start',{project:JSON.stringify(p)});const o=await call('operation-draft',{strategy:'circular_pocket'});o.depth=3.;o.settings.stock_clearance=.75;p.operations=[o];p.post.motion_output='circular';
   await call('session-start',{project:JSON.stringify(p)});const path=await call('generate');const native=await call('export');
   const bound=await call('simulate-current',{progress:1,resolution:128,budget:50000000,conservative:true});
   const a=await call('stock-preview',{progress:1,resolution:128,epoch:1,budget:50000000,conservative:true});
   const b=await call('stock-preview',{progress:1,resolution:128,epoch:1,budget:50000000,conservative:false});
   const rewind=await call('stock-preview',{progress:0,resolution:128,epoch:1,budget:50000000,conservative:true});
   const obj=await call('stock-obj',{progress:1,resolution:128,budget:50000000,conservative:true});
   p.post.motion_output='linear';await call('session-start',{project:JSON.stringify(p)});const linear=await call('export');
   p.post.motion_output='spline';await call('session-start',{project:JSON.stringify(p)});const spline=await call('export');
   return {arcs:path.analytical_arcs,native,linear,spline,bound:bound.summary,a:{reset:a.reset,summary:a.summary,vertices:a.vertices.length},b:{reset:b.reset,summary:b.summary},rewind:{reset:rewind.reset,summary:rewind.summary},obj:obj.slice(0,80)};
  } finally {worker.terminate()}
 });
 assert.ok(result.arcs>0);assert.match(result.native,/\nG[23] /);assert.match(result.native,/G17\nG91\.1/);assert.doesNotMatch(result.linear,/\nG[235] /);assert.match(result.spline,/\nG5 /);assert.ok(result.native.length<result.linear.length);
 assert.equal(result.bound.conservative,true);assert.ok(result.a.vertices>0);assert.equal(result.a.summary.remaining_upper_bound_mm3,result.bound.remaining_upper_bound_mm3);assert.equal(result.b.reset,true);assert.equal(result.b.summary.conservative,false);assert.equal(result.rewind.reset,true);assert.equal(result.rewind.summary.removed_volume_mm3,0);assert.match(result.obj,/remaining stock|^#|^o /i);
 await page.reload();await ready(page);assert.equal((await state(page)).operations[0].settings.stock_clearance,0.75);assert.equal((await state(page)).post.motion_output,'spline');assert.deepEqual(errors,[]);
 console.log('PASS: remaining-stock clearance save/reload, circular, spline and linear post output, conservative simulation, transferable playback, mode/rewind resets and matching stock export');
}finally{await browser.close()}
