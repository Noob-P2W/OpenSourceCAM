// Exercise CAD flattening without downloading or executing the kernel.
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
const source=readFileSync(new URL('./cad-worker.js',import.meta.url),'utf8');
async function flatten(meshes){const messages=[];const self={location:{href:'https://localhost/cad-worker.js'},postMessage:m=>messages.push(m)};vm.runInNewContext(source,{self,URL,importScripts(){},occtimportjs:async()=>({ReadStepFile:()=>({success:true,meshes})})});await self.onmessage({data:{name:'test.step',bytes:new ArrayBuffer(1)}});return messages.at(-1)}
function repeated(count,reverse=false){const positions=[],indices=[];for(let i=0;i<count;i++){const base=positions.length/3;positions.push(0,0,0,1,0,0,0,1,0);indices.push(...(reverse?[base,base+2,base+1]:[base,base+1,base+2]))}return{attributes:{position:{array:positions}},index:{array:indices}}}
const result=await flatten([repeated(60000),repeated(60000,true)]);assert.ok(!result.error,result.error);assert.equal(result.mesh.vertices.length,3);assert.equal(result.mesh.triangles.length,120000);assert.deepEqual(Array.from(result.mesh.triangles[0]),[0,1,2]);assert.deepEqual(Array.from(result.mesh.triangles[60000]),[0,2,1]);
const near=await flatten([{attributes:{position:{array:[0,0,0,1,0,0,0,1,0,1+1e-9,0,0]}},index:{array:[0,1,2,0,3,2]}}]);assert.equal(near.mesh.vertices.length,4,'Do not merge nearby but distinct positions');
const invalid=await flatten([{attributes:{position:{array:[0,0,0,1,0,0,0,1,0]}},index:{array:[0,1,999]}}]);assert.match(invalid.error,/invalid indices/);
const over=await flatten([{attributes:{position:{array:[0,0,0,1,0,0,0,1,0]}},index:{array:new Uint32Array(250001*3)}}]);assert.match(over.error,/250,000/);assert.equal(over.mesh,undefined);
console.log('PASS: CAD exact vertex sharing, 120,000 preserved faces, winding, distinct small details and bounded failure');
