import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {ready,importModel,closeDialogs} from './test-ui.mjs';
const n=32,w=100,h=60,d=20,triangles=[];
for(const [o,u,v] of [ [[0,0,0],[0,h,0],[w,0,0]],[[0,0,d],[w,0,0],[0,h,0]],[[0,0,0],[0,0,d],[0,h,0]],[[w,0,0],[0,h,0],[0,0,d]],[[0,0,0],[w,0,0],[0,0,d]],[[0,h,0],[0,0,d],[w,0,0]] ]){
 const p=(x,y)=>o.map((c,i)=>c+u[i]*x/n+v[i]*y/n);
 for(let x=0;x<n;x++)for(let y=0;y<n;y++)triangles.push([p(x,y),p(x+1,y),p(x+1,y+1)],[p(x,y),p(x+1,y+1),p(x,y+1)]);
}
const buffer=Buffer.alloc(84+triangles.length*50);buffer.writeUInt32LE(triangles.length,80);triangles.forEach((t,i)=>t.flat().forEach((c,j)=>buffer.writeFloatLE(c,84+i*50+12+j*4)));
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try{for(const software of [false,true]){
 const context=await browser.newContext({viewport:{width:390,height:844}});
 await context.addInitScript(software=>{const Original=Worker;window.Worker=class extends Original{constructor(...args){super(...args);this.addEventListener('message',({data})=>{if(data.value?.workspace_preview?.length)window.fixturePreview=data.value.workspace_preview[0]})}};if(software){const get=HTMLCanvasElement.prototype.getContext;HTMLCanvasElement.prototype.getContext=function(type,...args){return type==='webgl'?null:get.call(this,type,...args)}}},software);
 const page=await context.newPage();await page.goto(process.env.PWA_URL);await ready(page);await importModel(page,{name:'closed-dense-fixture.stl',mimeType:'application/octet-stream',buffer},'vise');await closeDialogs(page);await page.locator('#fit-all').click();
 await page.waitForFunction(count=>Number(document.querySelector('#canvas').dataset.referenceTriangles)===count,triangles.length);
 const preview=await page.evaluate(()=>({triangles:window.fixturePreview.triangles.length,vertices:window.fixturePreview.vertices.length}));assert.equal(preview.triangles,triangles.length);assert.ok(preview.vertices<triangles.length,'Retain shared vertices instead of expanding every triangle');
 if(software){await page.locator('#view').selectOption('top');await page.screenshot({path:'/tmp/vise-software-top.png'});const coverage=await page.locator('#canvas').evaluate(canvas=>{const p=canvas.getContext('2d').getImageData(0,0,canvas.width,canvas.height),brown=(x,y)=>{const i=(y*p.width+x)*4;return Math.abs(p.data[i]-140)<3&&Math.abs(p.data[i+1]-130)<3&&Math.abs(p.data[i+2]-104)<3};let x0=p.width,y0=p.height,x1=0,y1=0;for(let y=0;y<p.height;y++)for(let x=0;x<p.width;x++)if(brown(x,y)){x0=Math.min(x0,x);y0=Math.min(y0,y);x1=Math.max(x1,x);y1=Math.max(y1,y)}let solid=0,total=0;for(let y=y0+2;y<y1-2;y++)for(let x=x0+2;x<x1-2;x++){total++;if(brown(x,y))solid++}return solid/total});assert.ok(coverage>.97,'The flat fixture face must remain solid, without dropped triangles or grid showing through it');}
 await page.screenshot({path:`/tmp/vise-solid-${software?'software':'webgl'}.png`});console.log(`PASS: ${software?'Software':'WebGL'} retains all ${triangles.length} closed fixture faces with shared vertices`);await context.close();
}}finally{await browser.close()}
