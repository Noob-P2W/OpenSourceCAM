import {visibility} from './test-ui.mjs';
import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {ready,section,closeDialogs,state,generate,acceptNextConfirmation} from './test-ui.mjs';

const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox','--enable-unsafe-swiftshader']});
async function visibleToolPixels(page){
 return page.locator('#canvas').evaluate(canvas=>new Promise(resolve=>{
  const toggle=document.getElementById('show-tool');
  const capture=()=>{const gl=canvas.getContext('webgl');if(!gl)return canvas.getContext('2d').getImageData(0,0,canvas.width,canvas.height).data;const pixels=new Uint8Array(canvas.width*canvas.height*4);gl.readPixels(0,0,canvas.width,canvas.height,gl.RGBA,gl.UNSIGNED_BYTE,pixels);return pixels};
  toggle.checked=false;toggle.dispatchEvent(new Event('input'));
  requestAnimationFrame(()=>{const without=capture();toggle.checked=true;toggle.dispatchEvent(new Event('input'));requestAnimationFrame(()=>{const withTool=capture();let changed=0;for(let i=0;i<withTool.length;i+=4)if(Math.abs(without[i]-withTool[i])+Math.abs(without[i+1]-withTool[i+1])+Math.abs(without[i+2]-withTool[i+2])>30)changed++;resolve(changed)})});
 }));
}
try{
 for(const [width,height,software] of [[390,844,false],[768,1024,false],[1440,900,false],[390,844,true]]){
  const context=await browser.newContext({viewport:{width,height},hasTouch:true});
  await context.addInitScript(()=>{
   const Original=Worker;window.toolRequests=[];
   window.Worker=class extends Original{
    constructor(...args){super(...args);this.addEventListener('message',({data})=>{if(data.value?.moves)window.generatedPath=data.value;if(data.value?.format_version&&data.value?.operations?.length)window.seedProject=data.value})}
    postMessage(message,transfer){if(message.action==='playback-tool')window.toolRequests.push(message.index);super.postMessage(message,transfer)}
   };
  });
  if(software)await context.addInitScript(()=>{const get=HTMLCanvasElement.prototype.getContext;HTMLCanvasElement.prototype.getContext=function(type,...args){return type==='webgl'?null:get.call(this,type,...args)}});
  const page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(process.env.PWA_URL||'http://127.0.0.1:8098');await ready(page);const seed=await page.evaluate(()=>window.seedProject);
  await page.locator('[data-process=milling]').click();await page.locator('#status').filter({hasText:'Milling workspace opened'}).waitFor();
  await section(page,'tools');await page.locator('#tool-table tr').first().getByRole('button',{name:'Edit',exact:true}).click();
  const editor=page.locator('#tool-library-editor');
  for(const [kind,count] of [['drill',2],['holder',3]]){
   await editor.getByLabel('New tool type',{exact:true}).selectOption(kind);await editor.getByRole('button',{name:'Add tool',exact:true}).click();
   await page.waitForFunction(count=>JSON.parse(localStorage.getItem('oc-project')).tool_library.tools.length===count,count);
  }
  await closeDialogs(page);const project=await state(page),[mill,drill,holder]=project.tool_library.tools;
  mill.name='Measured 4 mm end mill';mill.holder=holder.id;Object.assign(mill.geometry,{diameter:4,shaft_diameter:6,flute_length:14,shoulder_length:14,overall_length:63,stickout:35,gauge_length:75});
  drill.name='Measured 8 mm drill';Object.assign(drill.geometry,{diameter:8,shaft_diameter:8,flute_length:20,shoulder_length:20,overall_length:70,stickout:40,gauge_length:70});
  Object.assign(holder.geometry,{body_diameter:20,overall_length:20,clamping_length:15});
  for(const [index,tool] of [mill,drill].entries()){tool.post.length_offset=index+1;tool.post.pocket=index+1}
  Object.assign(project.machine.tool_changer,{mode:'manual',manual_position:[10,10,25],controller_managed:false});project.post.processor='mach3';project.active_tool=mill.id;project.active_preset=mill.default_preset;
  const pocket=structuredClone(seed.operations[0]);pocket.tool_id=null;pocket.preset_id=null;
  const hole=structuredClone(pocket);Object.assign(hole,{name:'Drill the hole',tool_id:drill.id,strategy:'drill_once',x:90,y:60,depth:4});
  project.operations=[pocket,hole,{...structuredClone(pocket),name:'Return to the end mill',tool_id:mill.id}];
  await section(page,'project');void acceptNextConfirmation(page);await page.locator('#open').setInputFiles({name:'moving-tools.cam.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(project))});
  await page.locator('#status').filter({hasText:'Project opened'}).waitFor().catch(async error=>{throw new Error(await page.locator('#status').textContent(),{cause:error})});await generate(page);await page.locator('.playback-dock > summary').click();
  const canvas=page.locator('#canvas');await page.waitForFunction(()=>document.querySelector('#canvas').dataset.toolId==='1');
  const seek=async(index,fraction=.5)=>{
   const target=await page.evaluate(({index,fraction})=>{const path=window.generatedPath,move=path.moves.find(m=>m.operation===index&&!m.rapid);return {percent:(move.start_seconds+(move.end_seconds-move.start_seconds)*fraction)/path.seconds*100,tip:move.from.map((v,i)=>v+(move.to[i]-v)*fraction)}},{index,fraction});
   await page.locator('#progress').evaluate((input,value)=>{input.value=String(value);input.dispatchEvent(new Event('input',{bubbles:true}))},target.percent);
   await page.waitForFunction(index=>document.querySelector('#canvas').dataset.toolOperation===String(index),index);
   await page.waitForFunction(id=>document.querySelector('#canvas').dataset.toolId===String(id),index===1?drill.id:mill.id);
   await page.waitForFunction(expected=>document.querySelector('#canvas').dataset.toolTip.split(',').every((v,i)=>Math.abs(Number(v)-expected[i])<1e-7),target.tip);
   const tip=(await canvas.getAttribute('data-tool-tip')).split(',').map(Number);for(let i=0;i<3;i++)assert.ok(Math.abs(tip[i]-target.tip[i])<1e-7,'The assembly tip must follow the interpolated path exactly');
   return target;
  };
  await seek(0);assert.match(await page.locator('#playback-tool-state').textContent(),/T1.*Measured 4 mm end mill/);
  assert.ok(Number(await canvas.getAttribute('data-tool-triangles'))>200,'Draw the assembly surface, not a point');
  assert.ok(await visibleToolPixels(page)>100,'The actual tool must occupy visibly more than a cursor dot');
  await visibility(page,'show-simulation',true);await page.locator('#stock-playback-state').filter({hasText:'playback preview'}).waitFor();
  const sceneBuilds=await canvas.getAttribute('data-scene-builds');await page.locator('#playback-speed').selectOption('128');await page.locator('#play').click();await page.waitForTimeout(700);
  const synchronized=await canvas.evaluate(canvas=>{const percent=Number(canvas.dataset.stockProgress),time=percent/100*window.generatedPath.seconds,path=window.generatedPath,move=path.moves.find(m=>m.end_seconds>time)||path.moves.at(-1),f=Math.max(0,Math.min(1,(time-move.start_seconds)/(move.end_seconds-move.start_seconds)));return {display:Number(canvas.dataset.displayProgress),stock:percent,actual:canvas.dataset.toolTip.split(',').map(Number),expected:move.from.map((v,i)=>v+(move.to[i]-v)*f)}});
  assert.equal(await canvas.getAttribute('data-scene-builds'),sceneBuilds,'Stock updates must retain model, path and reference GPU buffers');
  assert.ok(Math.abs(synchronized.display-synchronized.stock)<1e-8,'The cutter must never outrun its displayed stock snapshot');for(let i=0;i<3;i++)assert.ok(Math.abs(synchronized.actual[i]-synchronized.expected[i])<1e-6,'Rendered tool tip must match the processed stock time');
  await page.locator('#play').click();await page.locator('#playback-speed').selectOption('1');
  assert.equal(await page.locator('.playback-controls').evaluate(root=>{const host=root.closest('.viewer').getBoundingClientRect();return [...root.children].every(el=>{const rect=el.getBoundingClientRect();return rect.left>=host.left&&rect.right<=host.right+1&&el.scrollWidth<=el.clientWidth+1})}),true,'All playback controls must fit the device width');
  const cutting=await seek(1,.8);assert.match(await page.locator('#playback-tool-state').textContent(),/T2.*Measured 8 mm drill/);assert.ok(await visibleToolPixels(page)>100);await page.locator('#stock-playback-state').filter({hasText:`${cutting.percent.toFixed(1)}%`}).waitFor();
  await page.screenshot({path:`/tmp/cam-moving-tool-${width}-${software?'software':'webgl'}.png`});
  await seek(2);await seek(0);const requests=await page.evaluate(()=>window.toolRequests.length);assert.equal(requests,2,'Operations using the same library assembly must reuse its mesh');
  await page.locator('#play').click();await page.waitForTimeout(180);await page.locator('#play').click();assert.equal(await canvas.getAttribute('data-tool-id'),'1');assert.equal(await page.evaluate(()=>window.toolRequests.length),requests,'Playback frames must not request new assemblies');
  await page.locator('#selected-path').setChecked(true);await seek(0);await page.locator('#progress').evaluate((input,value)=>{input.value=String(value);input.dispatchEvent(new Event('input',{bubbles:true}))},await page.evaluate(()=>{const p=window.generatedPath,m=p.moves.find(m=>m.operation===1);return (m.start_seconds+m.end_seconds)/2/p.seconds*100}));
  await page.waitForFunction(()=>document.querySelector('#canvas').dataset.toolTriangles==='0');assert.equal(await page.locator('#playback-tool-state').textContent(),'');await page.locator('#selected-path').setChecked(false);await seek(1);
  if(!software){
   const retainedStock=await canvas.getAttribute('data-stock-triangles');await canvas.evaluate(canvas=>{window.toolContext=canvas.getContext('webgl').getExtension('WEBGL_lose_context');window.toolContext.loseContext()});await page.locator('#status').filter({hasText:'3D graphics paused'}).waitFor();await canvas.evaluate(()=>window.toolContext.restoreContext());await page.locator('#status').filter({hasText:'3D view restored'}).waitFor();await page.waitForFunction(()=>Number(document.querySelector('#canvas').dataset.toolTriangles)>200);assert.equal(await canvas.getAttribute('data-tool-id'),'2');assert.equal(await canvas.getAttribute('data-stock-triangles'),retainedStock,'Graphics recovery must restore every retained stock strip');assert.ok(await visibleToolPixels(page)>100,'Restored graphics must actually draw the tool');
  }
  assert.deepEqual(errors,[]);console.log(`PASS: ${width}px ${software?'software':'WebGL'} library assemblies, holder, tip alignment, visible tool pixels, tool changes, stock playback, bounded reuse, filtering and graphics recovery`);await context.close();
 }
}finally{await browser.close()}
