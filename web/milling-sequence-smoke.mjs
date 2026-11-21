import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {ready,workspace,state,acceptNextConfirmation,generate,visibility} from './test-ui.mjs';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try{
 const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[];page.on('pageerror',e=>errors.push(e.message));await page.addInitScript(()=>{const Original=Worker;window.Worker=class extends Original{constructor(...args){super(...args);this.addEventListener('message',({data})=>{if(data.value?.moves)window.sequencePath=data.value})}}});await page.goto(process.env.PWA_URL);await ready(page);await workspace(page);
 const base=await state(page);
 const fixture=await page.evaluate(async p=>{
  p.part=null;p.workspace=[];p.operations=[];p.stock={...p.stock,width:50,height:40,thickness:8};
  const worker=new Worker('./worker.js',{type:'module'});let id=0;const call=(action,args={})=>new Promise((resolve,reject)=>{const n=++id;const listen=({data})=>{if(data.id!==n)return;worker.removeEventListener('message',listen);data.error?reject(Error(data.error)):resolve(data.value)};worker.addEventListener('message',listen);worker.postMessage({id:n,action,...args})});
  try{await call('session-start',{project:JSON.stringify(p)});const initialized=await call('session-edit',{edit:{kind:'initialize_tools'}});Object.assign(p,initialized.project);const rough=await call('operation-draft',{strategy:'pocket'});Object.assign(rough,{x:5,y:5,width:40,height:30,depth:3});rough.settings.pocket_pattern='offset';rough.settings.step_down=1;const finish=await call('operation-draft',{strategy:'surface_smooth'});Object.assign(finish,{x:13,y:12,width:24,height:16});finish.settings.height_offset=-3;
   const edited=await call('session-edit',{edit:{kind:'add_library_tool',tool_kind:'drill'}});const tools=edited.project.tool_library;
   return {planar:{...p,operations:[rough,finish]},multi:{...p,tool_library:tools,operations:[rough,{...rough,name:'Prepared drill',strategy:'drill_once',tool_id:tools.tools.at(-1).id,x:25,y:20,depth:3,settings:{...rough.settings,pocket_pattern:'raster'}}]}};
  }finally{worker.terminate()}
 },base);
 async function open(p){void acceptNextConfirmation(page);await page.locator('#open').setInputFiles({name:'sequence-check.cam.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(p))});await page.locator('#status').filter({hasText:'Project opened'}).waitFor().catch(async e=>{throw Error(await page.locator('#status').textContent(),{cause:e})});await generate(page)}
 await open(fixture.planar);const metrics=await page.evaluate(()=>({plunges:window.sequencePath.moves.filter(m=>m.operation===1&&!m.rapid&&m.to[2]<m.from[2]).length,changes:window.sequencePath.moves.filter(m=>m.tool_change).length}));assert.ok(metrics.plunges<=1);assert.equal(metrics.changes,0);
 await visibility(page,'show-simulation',true);await page.waitForFunction(()=>document.getElementById('canvas').dataset.stockRoles==='0,1,2');assert.equal(await page.locator('.stock-legend:visible [data-role=finish]').textContent(),'Finish pass');
 const multi=fixture.multi;multi.machine.tool_changer.mode='manual';multi.machine.tool_changer.manual_position=[10,10,20];multi.machine.tool_changer.change_seconds=4;multi.tool_library.tools.forEach((t,i)=>t.post.length_offset=i+1);await open(multi);
 const pause=await page.evaluate(()=>{const p=window.sequencePath,m=p.moves.find(m=>m.tool_change);return{percent:(m.start_seconds+2)/p.seconds*100,from:m.from,known:m.change_position_known}});assert.deepEqual(pause.from,[10,10,20]);assert.equal(pause.known,true);
 await page.locator('#progress').evaluate((el,value)=>{el.value=String(value);el.dispatchEvent(new Event('input',{bubbles:true}))},pause.percent);await page.waitForFunction(()=>document.getElementById('canvas').dataset.toolChange==='true');assert.match(await page.locator('#playback-tool-state').textContent(),/Tool exchange.*configured change station/);assert.equal(await page.locator('#canvas').getAttribute('data-tool-triangles'),'0');
 const overlay=await page.evaluate(async()=>{
  const {Viewport}=await import('./viewport.js');const c=document.createElement('canvas');c.style.cssText='position:fixed;left:0;top:0;width:320px;height:320px';document.body.append(c);const v=new Viewport(c,()=>{},()=>{});v.controls={axes:false,grid:false};v.setView('top');
  const mesh={vertices:[[0,0,0],[20,0,0],[20,20,0],[0,20,0],[9,9,2],[11,9,2],[11,11,2],[9,11,2]],triangles:[[0,1,2],[0,2,3],[4,5,6],[4,6,7]]};
  const p={stock:{width:20,height:20,thickness:5},part:{visible:true,units:'millimetres',offset:[0,0,0],geometry:{mesh}},operations:[{strategy:'surface_smooth'}]};const path={seconds:1,moves:[{from:[3,10,0],to:[17,10,0],operation:0,rapid:false,start_seconds:0,end_seconds:1}]};v.draw(p,[],path,100,null,null,false,false);
  const pixels=point=>{const [x,y]=v.projectWorld(point),data=new Uint8Array(5*5*4);v.gl.readPixels(Math.round(x)-2,c.height-Math.round(y)-2,5,5,v.gl.RGBA,v.gl.UNSIGNED_BYTE,data);let count=0;for(let i=0;i<data.length;i+=4)if(data[i]<15&&Math.abs(data[i+1]-107)<20&&Math.abs(data[i+2]-84)<20)count++;return count};
  const result={onFace:pixels([5,10,0]),behindBoss:pixels([10,10,0])};v.three.dispose();c.remove();return result;
 });assert.ok(overlay.onFace>0,'Coplanar finishing cuts must remain visible');assert.equal(overlay.behindBoss,0,'Raised geometry must still occlude hidden cuts');
 assert.deepEqual(errors,[]);console.log('PASS: same-tool finishing stays down, sampled rough/finish stock colors, configured tool-change travel and timed exchange in playback');
}finally{await browser.close()}
