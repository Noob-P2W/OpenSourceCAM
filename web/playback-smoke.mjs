import {visibility} from './test-ui.mjs';
import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {ready,generate,editOperation,saveOperation} from './test-ui.mjs';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try{
 for(const [width,height,software] of [[390,844,false],[768,1024,false],[1440,900,false],[390,844,true]]){
  const context=await browser.newContext({viewport:{width,height},hasTouch:true});
  if(software)await context.addInitScript(()=>{const get=HTMLCanvasElement.prototype.getContext;HTMLCanvasElement.prototype.getContext=function(type,...args){return type==='webgl'?null:get.call(this,type,...args)}});
  const page=await context.newPage(),errors=[];await page.addInitScript(()=>{const Original=Worker;window.Worker=class extends Original{constructor(...args){super(...args);this.addEventListener('message',({data})=>{if(data.value?.moves)window.testGeneratedPath=data.value})}}});page.on('pageerror',e=>errors.push(e.message));
  await page.goto(process.env.PWA_URL||'http://127.0.0.1:8096');await ready(page);
  assert.equal(await page.locator('#play').isDisabled(),true,'Playback needs generated paths');
  await generate(page);await page.locator('.playback-dock > summary').click();
  await page.clock.install({time:new Date('2026-11-10T12:00:00Z')});
  await page.clock.pauseAt(new Date('2026-11-10T12:00:01Z'));
  const seconds=await page.evaluate(()=>window.testGeneratedPath.seconds);const advance=(milliseconds,speed=1)=>milliseconds/1000/seconds*100*speed;
  const progress=async()=>Number(await page.locator('#progress').inputValue());
  const seek=async value=>{await page.locator('#progress').evaluate((input,value)=>{input.value=String(value);input.dispatchEvent(new Event('input',{bubbles:true}))},value);await page.clock.runFor(32)};
  await page.locator('#play').click();await page.clock.runFor(500);
  assert.ok(Math.abs(await progress()-advance(500))<advance(40),`Playback should advance at normal frame rates, got ${await progress()}%`);
  await page.locator('#play').click();const paused=await progress();await page.clock.runFor(500);assert.equal(await progress(),paused);
  await page.locator('#play').click();assert.equal(await progress(),paused,'Resume must keep the paused position');await page.clock.runFor(300);assert.ok(await progress()>paused);
  await page.locator('#play').click();await page.locator('#playback-speed').selectOption('2');const speedStart=await progress();await page.locator('#play').click();await page.clock.runFor(300);assert.ok(Math.abs(await progress()-speedStart-advance(300,2))<advance(40,2),'2× speed must double playback progress');await page.locator('#play').click();await page.locator('#playback-speed').selectOption('0.5');const slowStart=await progress();await page.locator('#play').click();await page.clock.runFor(300);assert.ok(Math.abs(await progress()-slowStart-advance(300,.5))<advance(40,.5),'0.5× speed must halve playback progress');await page.locator('#playback-speed').selectOption('1');
  await seek(60.5);assert.equal(await page.locator('#play').getAttribute('aria-pressed'),'false');await page.clock.runFor(500);assert.equal(await progress(),60.5,'Scrubbing pauses at the requested fraction');
  await page.locator('#play').click();await page.clock.runFor(300);assert.ok(await progress()>60.5);
  // Pick two times inside the first Rust-supplied move. The cursor interpolates
  // even before a complete segment has been drawn, in GPU and software rendering.
  const firstMovePercent=await page.evaluate(()=>window.testGeneratedPath.moves[0].end_seconds/window.testGeneratedPath.seconds*100);assert.ok(firstMovePercent>0);await seek(firstMovePercent*.2);const first=await page.locator('#canvas').screenshot();await seek(firstMovePercent*.6);assert.notDeepEqual(await page.locator('#canvas').screenshot(),first,'The tool cursor must visibly move within a segment');
  await seek(99.5);await page.locator('#play').click();await page.clock.runFor(Math.ceil(seconds*5+100));assert.equal(await progress(),100);assert.equal(await page.locator('#play').textContent(),'Play paths');
  await page.locator('#play').click();assert.equal(await progress(),0,'Replay restarts a completed preview');await page.clock.runFor(150);assert.ok(await progress()>0);
  await seek(40.5);await page.locator('#play').evaluate(button=>{button.click();button.click();button.click()});await page.clock.runFor(300);assert.ok(Math.abs(await progress()-40.5-advance(300))<advance(40),'Rapid toggles must leave only one animation loop');
  // Navigation and path visibility are presentation actions, not playback controls.
  const box=await page.locator('#canvas').boundingBox();await page.mouse.move(box.x+box.width*.5,box.y+box.height*.5);await page.mouse.down({button:'right'});await page.mouse.move(box.x+box.width*.5+20,box.y+box.height*.5+12);await page.mouse.up({button:'right'});await page.clock.runFor(150);assert.equal(await page.locator('#play').getAttribute('aria-pressed'),'true','Panning must not pause playback');
  await visibility(page,'show-paths',false);await page.clock.runFor(100);assert.equal(await page.locator('#play').getAttribute('aria-pressed'),'true');await visibility(page,'show-paths',true);
  await page.locator('#home-button').click();const left=await progress();await page.clock.runFor(500);assert.equal(await progress(),left,'Leaving the workspace stops playback');await page.locator('#resume-workspace').click();
  await page.locator('#play').click();await editOperation(page);await page.getByLabel('depth · mm',{exact:true}).fill('4');await saveOperation(page);assert.equal(await page.locator('#play').isDisabled(),true);assert.equal(await page.locator('#play').getAttribute('aria-pressed'),'false');
  const edited=await progress();await page.clock.runFor(500);assert.equal(await progress(),edited,'Edits invalidate paths and stop their animation');await generate(page);assert.equal(await page.locator('#play').isEnabled(),true);
  assert.deepEqual(errors,[]);console.log(`PASS: ${width}px ${software?'software':'WebGL'} playback, visible motion, pause/resume, scrub, completion/replay, single animation loop and invalidation`);await context.close();
 }
}finally{await browser.close()}
