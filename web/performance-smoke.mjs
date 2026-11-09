import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {section} from './test-ui.mjs';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox']});
try{
 const context=await browser.newContext({viewport:{width:390,height:844},deviceScaleFactor:3});
 await context.addInitScript(()=>{const stroke=CanvasRenderingContext2D.prototype.stroke;window.strokeCount=0;CanvasRenderingContext2D.prototype.stroke=function(...args){window.strokeCount++;return stroke.apply(this,args)}});
 const page=await context.newPage();page.setDefaultTimeout(20000);await page.goto(process.env.PWA_URL||'http://127.0.0.1:8081');await page.locator('#status').filter({hasText:'Ready'}).waitFor();
 await section(page,'import');
 const nx=250,ny=200,count=nx*ny*2,bytes=Buffer.alloc(84+count*50);bytes.writeUInt32LE(count,80);let face=0;
 const vertex=(x,y)=>[x,y,Math.fround(Math.sin(x/30)*Math.cos(y/30))];
 for(let y=0;y<ny;y++)for(let x=0;x<nx;x++)for(const tri of [[vertex(x,y),vertex(x+1,y),vertex(x,y+1)],[vertex(x+1,y),vertex(x+1,y+1),vertex(x,y+1)]]){let i=0;for(const v of tri.flat()){bytes.writeFloatLE(v,84+face*50+12+i*4);i++}face++}
 page.once('dialog',d=>d.accept());const start=Date.now();await page.locator('#model').setInputFiles({name:'dense-100k.stl',mimeType:'application/octet-stream',buffer:bytes});await page.locator('#status').filter({hasText:'Model imported'}).waitFor();const importMs=Date.now()-start;
 assert.equal(await page.locator('#json').inputValue(),'','Large mesh must not be expanded into the hidden text editor');
 await page.locator('#preview-detail').filter({hasText:'Full geometry is retained'}).waitFor();
 const strokes=await page.evaluate(()=>window.strokeCount);assert.ok(strokes<40,`Too many canvas strokes: ${strokes}`);
 assert.ok(await page.locator('#canvas').evaluate(c=>c.width*c.height)<=4000000);
 await section(page,'setup');await page.getByLabel('Stock X · mm',{exact:true}).fill('270');await page.getByLabel('Stock X · mm',{exact:true}).blur();await page.waitForFunction(()=>document.getElementById('undo').disabled===false);await page.locator('#undo').click();await page.waitForFunction(()=>document.querySelector('#setup input[type=number]').value==='260');
 await section(page,'project');const event=page.waitForEvent('download');await page.locator('#save').click();const download=await event;const saved=JSON.parse(readFileSync(await download.path(),'utf8'));assert.equal(saved.part.geometry.mesh.triangles.length,100000);assert.equal(saved.part.geometry.mesh.vertices.length,(nx+1)*(ny+1));
 for(let i=0;i<2;i++){
  await page.keyboard.press('Escape');await section(page,'import');
  await page.waitForFunction(()=>!document.getElementById('model').disabled);
  page.once('dialog',d=>d.accept());await page.locator('#model').setInputFiles({name:`dense-repeat-${i}.stl`,mimeType:'application/octet-stream',buffer:bytes});await page.locator('#status').filter({hasText:'Model imported'}).waitFor();assert.equal(await page.locator('#json').inputValue(),'');
 }
 assert.ok(importMs<20000,`Dense STL import took ${importMs}ms`);
 console.log(`PASS: 100,000-triangle STL imported in ${importMs} ms, ${strokes} strokes, exact vertex sharing, full-resolution save, bounded canvas, lazy editor, undo after edit, repeated large imports`);
 await context.close();
}finally{await browser.close()}
