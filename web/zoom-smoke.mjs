import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {ready,workspace} from './test-ui.mjs';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try{for(const software of [false,true]){
 const context=await browser.newContext({viewport:{width:1100,height:1000},hasTouch:true});if(software)await context.addInitScript(()=>{const get=HTMLCanvasElement.prototype.getContext;HTMLCanvasElement.prototype.getContext=function(type,...args){return type==='webgl'?null:get.call(this,type,...args)}});
 const page=await context.newPage();await page.goto(process.env.PWA_URL);await ready(page);await workspace(page);const canvas=page.locator('#canvas');const box=await canvas.boundingBox();
 await page.mouse.move(box.x+box.width/2,box.y+box.height/2);await page.mouse.down({button:'right'});await page.mouse.move(box.x+box.width/2+100,box.y+box.height/2+60,{steps:5});await page.mouse.up({button:'right'});
 const point=[box.width*.35,box.height*.4];const world=await canvas.evaluate((c,p)=>c.camViewport.pickRay(...p).origin,point);await page.mouse.move(box.x+point[0],box.y+point[1]);await page.mouse.wheel(0,-500);await page.waitForFunction(()=>Number(document.querySelector('#canvas').dataset.zoom)>1.5);
 async function projected(world){await page.waitForTimeout(50);return canvas.evaluate((c,w)=>c.camViewport.projectWorld(w).slice(0,2),world)}
 const near=(a,b)=>assert.ok(Math.hypot(a[0]-b[0],a[1]-b[1])<1,JSON.stringify({actual:a,expected:b}));near(await projected(world),point);
 const center=[box.width/2,box.height/2],centerWorld=await canvas.evaluate((c,p)=>c.camViewport.pickRay(...p).origin,center);await page.getByRole('button',{name:'Zoom in',exact:true}).click();near(await projected(centerWorld),center);await page.getByRole('button',{name:'Zoom out',exact:true}).click();near(await projected(centerWorld),center);
 const session=await context.newCDPSession(page),cx=box.x+point[0],cy=box.y+point[1];const pinchWorld=await canvas.evaluate((c,p)=>c.camViewport.pickRay(...p).origin,point);
 await session.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{x:cx-25,y:cy,id:1},{x:cx+25,y:cy,id:2}]});await session.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{x:cx-40+15,y:cy+10,id:1},{x:cx+40+15,y:cy+10,id:2}]});await session.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});near(await projected(pinchWorld),[point[0]+15,point[1]+10]);
 // Reaching a zoom limit must not continue moving the view.
 await canvas.evaluate(c=>c.camViewport.magnify(100));await projected(world);const limit=await projected(world);await canvas.evaluate(c=>c.camViewport.magnify(2));near(await projected(world),limit);
 // Exercise orbit picking with a real visible surface, then change the
 // playback envelope without changing the user's framing.
 const stable=await page.evaluate(async software=>{
  const {Viewport}=await import('./viewport.js'),c=document.createElement('canvas');c.style.cssText='position:fixed;left:0;top:0;width:400px;height:400px';document.body.append(c);
  if(software){const get=c.getContext.bind(c);c.getContext=(type,...args)=>type==='webgl'?null:get(type,...args)}
  const mesh={vertices:[[0,0,0],[20,0,0],[20,20,0],[0,20,0]],triangles:[[0,1,2],[0,2,3]]};
  const project={stock:{width:20,height:20,thickness:5},part:{visible:true,units:'millimetres',offset:[0,0,0],geometry:{mesh}}};
  let v;const draw=()=>v.draw(project,[],null,0,null,null,false,true);v=new Viewport(c,draw,()=>{});v.controls={axes:false,grid:false};draw();
  const point=[7,8,0],screen=v.projectWorld(point),rect=c.getBoundingClientRect(),scale=v.viewScale;
  v.beginOrbit(rect.left+screen[0],rect.top+screen[1]);
  const immediateError=Math.hypot(...v.projectWorld(point).slice(0,2).map((x,i)=>x-screen[i])),ray=v.pickRay(...screen),t=point.reduce((n,x,i)=>n+(x-ray.origin[i])*ray.direction[i],0),tapRayError=Math.hypot(...point.map((x,i)=>x-ray.origin[i]-t*ray.direction[i]));
  v.yaw-=35*.012;v.elevation+=15*.012;draw();
  const error=Math.hypot(...v.projectWorld(point).slice(0,2).map((x,i)=>x-screen[i])),pivotError=Math.hypot(...v.cameraFrame.center.map((x,i)=>x-point[i]));
  const path={seconds:1,moves:[{from:[0,0,0],to:[500,500,200],operation:0,rapid:true,start_seconds:0,end_seconds:1}]};
  const tool={min:[-20,-20,0],max:[20,20,1000],meshes:[mesh],cutting_diameter:6};v.draw(project,[],path,50,{mesh},null,false,true,null,()=>tool);
  const scaleError=Math.abs(v.viewScale-scale),afterError=Math.hypot(...v.projectWorld(point).slice(0,2).map((x,i)=>x-screen[i]));
  // A layout change can precede the next paint; picking must already agree
  // with projection at the new canvas size.
  const beforeResize=v.projectWorld(point);c.style.height='340px';const resized=v.projectWorld(point),resizeProjectionError=Math.abs(resized[1]-beforeResize[1]+30),resizeRay=v.pickRay(...resized),rt=point.reduce((n,x,i)=>n+(x-resizeRay.origin[i])*resizeRay.direction[i],0),resizeRayError=Math.hypot(...point.map((x,i)=>x-resizeRay.origin[i]-rt*resizeRay.direction[i]));
  v.three?.dispose();c.remove();return{error,pivotError,scaleError,afterError,immediateError,tapRayError,resizeRayError,resizeProjectionError};
 },software);
 for(const [key,value] of Object.entries(stable))assert.ok(value<1e-5,`${key}: ${JSON.stringify(stable)}`);
 await context.close();console.log(`PASS: ${software?'software':'WebGL'} panned wheel/cursor, viewport-center buttons, moving pinch anchor and zoom limits`);
}}finally{await browser.close()}
