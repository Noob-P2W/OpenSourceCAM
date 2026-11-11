import {chromium} from 'playwright';
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
import {ready,importModel,state,section,acceptNextConfirmation} from './test-ui.mjs';
const browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH||'/usr/bin/chromium',args:['--no-sandbox','--enable-unsafe-swiftshader']});
try{
 const page=await browser.newPage({viewport:{width:390,height:844},hasTouch:true});await page.goto(process.env.PWA_URL);await ready(page);
 const source=readFileSync(new URL('../examples/models/mount.stl',import.meta.url));const count=source.readUInt32LE(80);assert.equal(source.length,84+50*count);
 const badFace=Buffer.alloc(50);for(let i=12;i<48;i+=4)badFace.writeFloatLE(99999,i);const repaired=Buffer.concat([source,badFace]);repaired.writeUInt32LE(count+1,80);
 await importModel(page,{name:'repair.stl',mimeType:'application/octet-stream',buffer:repaired});
 assert.match(await page.locator('#status').textContent(),/Auto-fixed 1 degenerate/);
 const p=await state(page);assert.equal(p.part.geometry.removed_degenerate_triangles,1);assert.equal(p.part.geometry.mesh.triangles.length,count);assert.ok(p.part.geometry.mesh.vertices.every(v=>v.every(c=>Math.abs(c)<100)), 'Remove unused outlier vertices left by repaired faces');
 await section(page,'setup');await page.locator('#mesh-repair-note').filter({hasText:'removed 1 zero-area'}).waitFor();
 const invalid=Buffer.alloc(134);invalid.writeUInt32LE(1,80);await section(page,'import');void acceptNextConfirmation(page);await page.locator('#model').setInputFiles({name:'unusable.stl',mimeType:'application/octet-stream',buffer:invalid});await page.locator('#status.error').filter({hasText:'no usable triangles'}).waitFor();assert.deepEqual(await state(page),p);
 // The browser CAD adapter uses the same Rust cleanup before committing a project.
 const cad=await page.evaluate(async()=>{const core=await import('./pkg/opensourcecam.js');await core.default();const p=JSON.parse(core.cad_project('repair.step','ISO-10303-21;','step',JSON.stringify({vertices:[[0,0,0],[1,0,0],[0,1,0]],triangles:[[0,1,2],[0,0,0]]})));return p.part.geometry});assert.equal(cad.removed_degenerate_triangles,1);assert.equal(cad.mesh.triangles.length,1);
 console.log('PASS: automatic STL and CAD cleanup, visible repair report, and unusable import preserves the workspace');
}finally{await browser.close()}
