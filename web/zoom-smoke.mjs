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
 await context.close();console.log(`PASS: ${software?'software':'WebGL'} panned wheel/cursor, viewport-center buttons, moving pinch anchor and zoom limits`);
}}finally{await browser.close()}
