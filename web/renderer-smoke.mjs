import {chromium} from 'playwright';
import assert from 'node:assert/strict';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try{
 const page=await browser.newPage();await page.goto(process.env.PWA_URL);await page.locator('#status').filter({hasText:'Ready'}).waitFor();
 const result=await page.evaluate(async()=>{
  const {ThreeView}=await import('./three-view.js');const canvas=document.createElement('canvas');canvas.width=200;canvas.height=200;document.body.append(canvas);const gl=canvas.getContext('webgl');const viewer=new ThreeView(canvas,gl);
  const empty=viewer.geometry({vertices:[],triangles:[]},'empty');const emptyFinite=empty.origin.every(Number.isFinite);
  const shape={vertices:[[1000000,1000000,0],[1000001,1000000,0],[1000000,1000001,0]],triangles:[[0,1,2]]};
  const a=viewer.mesh(shape,[.6,.7,.8],'a'),b=viewer.mesh({vertices:shape.vertices.map(p=>[p[0]+20,p[1]-10,p[2]]),triangles:shape.triangles},[.5,.6,.7],'b');
  const shares=a.cached===b.cached,originsDiffer=a.origin[0]!==b.origin[0];
  const marker=viewer.marker(),item={three:marker,lines:true,drawCount:1,drawColor:[1,.35,.08],drawOffset:[0,0,0]};
  const args={width:200,height:200,dpr:1,center:[0,0,0],depth:[0,0,1],up:[0,1,0],scale:30,pan:[0,0],radius:2,dark:false};viewer.render([item],args);
  const pixels=new Uint8Array(200*200*4);gl.readPixels(0,0,200,200,gl.RGBA,gl.UNSIGNED_BYTE,pixels);let orange=0;for(let i=0;i<pixels.length;i+=4)if(pixels[i]>220&&pixels[i+1]<150&&pixels[i+2]<60)orange++;
  viewer.render([],args);const hidden=!marker.object.visible;
  viewer.remove(a);viewer.remove(b);viewer.remove(marker);viewer.limit=0;viewer.prune();const released=viewer.cacheBytes===0&&viewer.entries.size===0;viewer.dispose();canvas.remove();return{emptyFinite,shares,originsDiffer,orange,hidden,released};
 });
 assert.ok(result.emptyFinite);assert.ok(result.shares&&result.originsDiffer,'Translated identical shapes share buffers but keep their own origins');assert.ok(result.orange>=25,'Cursor remains a visible point in top view');assert.ok(result.hidden,'Cursor must disappear when not requested');assert.ok(result.released,'Unused geometry, materials and cursor must be releasable');console.log('PASS: Three.js empty meshes, local origins, shared geometry, visible/hidden cursor and explicit resource eviction');
}finally{await browser.close()}
