import * as THREE from './vendor/three.module.js';

// Presentation only. Full CAM meshes and all physical transforms remain in Rust.
export class ThreeView {
 constructor(canvas,gl){
  this.renderer=new THREE.WebGLRenderer({canvas,context:gl,antialias:false,alpha:false,preserveDrawingBuffer:true});
  this.renderer.outputColorSpace=THREE.LinearSRGBColorSpace;
  this.renderer.toneMapping=THREE.NoToneMapping;
  this.scene=new THREE.Scene();this.camera=new THREE.OrthographicCamera();
  this.scene.add(new THREE.AmbientLight(0xffffff,.4));const light=new THREE.DirectionalLight(0xffffff,.6*Math.PI);light.position.set(.3,-.5,.8);this.scene.add(light);
  this.entries=new Set();this.cache=new Map();this.aliases=new Map();this.cacheBytes=0;this.uploads=0;this.hits=0;this.limit=64*1024*1024;
  this.flatShading=!!(this.renderer.capabilities.isWebGL2||gl.getExtension('OES_standard_derivatives'));
  this.uintIndices=!!(this.renderer.capabilities.isWebGL2||gl.getExtension('OES_element_index_uint'));
 }
 geometry(mesh,key){
  if(key&&this.aliases.has(key)){const result=this.aliases.get(key);result.cached.used=performance.now();this.hits++;return result}
  const remap=new Map(),positions=[],indices=new Uint32Array(mesh.triangles.length*3);let offset=0;
  for(const triangle of mesh.triangles)for(const vertex of triangle){let index=remap.get(vertex);if(index===undefined){index=positions.length/3;remap.set(vertex,index);positions.push(...mesh.vertices[vertex])}indices[offset++]=index}
  const origin=[0,1,2].map(a=>{let lo=Infinity,hi=-Infinity;for(let i=a;i<positions.length;i+=3){lo=Math.min(lo,positions[i]);hi=Math.max(hi,positions[i])}return positions.length?(lo+hi)/2:0});const pos=Float32Array.from(positions,(v,i)=>v-origin[i%3]);let hash=2166136261;for(const array of [new Uint32Array(pos.buffer),indices])for(let i=0;i<array.length;i++)hash=Math.imul(hash^array[i],16777619)>>>0;
  const bucket=`${hash}:${pos.length}:${indices.length}`;let entry;
  const equal=(a,b)=>a.length===b.length&&a.every((v,i)=>v===b[i]);
  for(const e of this.cache.get(bucket)||[])if(equal(e.positions,pos)&&equal(e.indices,indices)){entry=e;this.hits++;break}
  if(!entry){const geometry=new THREE.BufferGeometry();geometry.setAttribute('position',new THREE.BufferAttribute(pos,3));geometry.setIndex(new THREE.BufferAttribute(positions.length/3<=65535?new Uint16Array(indices):indices,1));geometry.computeVertexNormals();geometry.computeBoundingBox();geometry.computeBoundingSphere();
   entry={geometry,positions:pos,indices,refs:0,used:performance.now(),bytes:pos.byteLength+indices.byteLength+geometry.getAttribute('normal').array.byteLength};
   if(!this.flatShading||!this.uintIndices&&positions.length/3>65535){entry.geometry=geometry.toNonIndexed();entry.geometry.computeVertexNormals();geometry.dispose()}
   entry.bytes=pos.byteLength+indices.byteLength;
   for(const attribute of Object.values(entry.geometry.attributes))if(attribute.array!==pos)entry.bytes+=attribute.array.byteLength;
   if(entry.geometry.index&&entry.geometry.index.array!==indices)entry.bytes+=entry.geometry.index.array.byteLength;
   const list=this.cache.get(bucket)||[];list.push(entry);this.cache.set(bucket,list);this.cacheBytes+=entry.bytes;this.uploads++;
  }
  const result={cached:entry,origin};if(key){this.aliases.set(key,result);if(this.aliases.size>512)this.aliases.delete(this.aliases.keys().next().value)}return result;
 }
 mesh(mesh,color,key){const {cached,origin}=this.geometry(mesh,key);cached.refs++;cached.used=performance.now();const material=new THREE.MeshPhongMaterial({color:new THREE.Color().setRGB(...color),specular:0x000000,shininess:0,flatShading:this.flatShading,side:THREE.DoubleSide});const object=new THREE.Mesh(cached.geometry,material);this.scene.add(object);const entry={object,cached,origin};this.entries.add(entry);return entry}
 upload(data,color,lines=false){
  const geometry=new THREE.BufferGeometry();
  if(lines){const positions=new Float32Array(data.length/2);for(let i=0,j=0;i<data.length;i+=6){positions[j++]=data[i];positions[j++]=data[i+1];positions[j++]=data[i+2]}geometry.setAttribute('position',new THREE.BufferAttribute(positions,3))}
  else{const buffer=new THREE.InterleavedBuffer(data,6);geometry.setAttribute('position',new THREE.InterleavedBufferAttribute(buffer,3,0));geometry.setAttribute('normal',new THREE.InterleavedBufferAttribute(buffer,3,3))}
  const material=lines?new THREE.LineBasicMaterial({color:new THREE.Color().setRGB(...color)}):new THREE.MeshPhongMaterial({color:new THREE.Color().setRGB(...color),specular:0x000000,shininess:0,flatShading:this.flatShading,side:THREE.DoubleSide});
  const object=lines?new THREE.LineSegments(geometry,material):new THREE.Mesh(geometry,material);this.scene.add(object);const entry={object};this.entries.add(entry);return entry;
 }
 marker(){const geometry=new THREE.BufferGeometry();geometry.setAttribute('position',new THREE.Float32BufferAttribute([0,0,0],3));const material=new THREE.PointsMaterial({color:new THREE.Color().setRGB(1,.35,.08),size:8,sizeAttenuation:false,depthTest:false});const object=new THREE.Points(geometry,material);object.renderOrder=10;this.scene.add(object);const entry={object,marker:true};this.entries.add(entry);return entry}
 remove(entry){if(!entry||!this.entries.delete(entry))return;this.scene.remove(entry.object);entry.object.material.dispose();if(entry.cached)entry.cached.refs--;else entry.object.geometry.dispose()}
 prune(){if(this.cacheBytes<=this.limit)return;const unused=[...this.cache.values()].flat().filter(e=>!e.refs).sort((a,b)=>a.used-b.used);for(const e of unused){if(this.cacheBytes<=this.limit)break;e.geometry.dispose();this.cacheBytes-=e.bytes;for(const [key,list] of this.cache){const next=list.filter(v=>v!==e);if(next.length)this.cache.set(key,next);else this.cache.delete(key)}for(const [key,value] of this.aliases)if(value.cached===e)this.aliases.delete(key)}}
 render(items,{width,height,dpr,center,depth,up,scale,pan,radius,dark}){
  const size=`${width}:${height}:${dpr}`;if(this.size!==size){this.size=size;this.renderer.setPixelRatio(dpr);this.renderer.setSize(width,height,false)}this.renderer.setClearColor(dark?0x0e171f:0xf2f5f6,1);
  this.camera.left=(-width/2-pan[0])/scale;this.camera.right=(width/2-pan[0])/scale;this.camera.top=(height/2+pan[1])/scale;this.camera.bottom=(-height/2+pan[1])/scale;
  this.camera.near=Math.max(.000001,radius*.00001);this.camera.far=radius*8;this.camera.position.fromArray(center).addScaledVector(new THREE.Vector3(...depth),radius*4);this.camera.up.fromArray(up);this.camera.lookAt(...center);this.camera.updateProjectionMatrix();this.camera.updateMatrixWorld();
  for(const e of this.entries)e.object.visible=false;
  for(const item of items){const entry=item.three;if(!entry)continue;const object=entry.object;object.visible=true;object.material.color.setRGB(...item.drawColor);if(!item.lines)object.material.wireframe=!!item.wireframe;object.geometry.setDrawRange(0,item.drawCount);object.matrixAutoUpdate=false;object.matrix.identity();if(item.matrix)object.matrix.fromArray(item.matrix);if(entry.origin)object.matrix.multiply(new THREE.Matrix4().makeTranslation(...entry.origin));object.matrix.elements[12]+=item.drawOffset[0];object.matrix.elements[13]+=item.drawOffset[1];object.matrix.elements[14]+=item.drawOffset[2];object.renderOrder=entry.marker?10:item.selection?1:item.lines?2:0;object.material.polygonOffset=!!item.selection;object.material.polygonOffsetFactor=-1;object.material.polygonOffsetUnits=-1;}
  this.renderer.render(this.scene,this.camera);this.prune();
 }
 dispose(){for(const entry of [...this.entries])this.remove(entry);for(const list of this.cache.values())for(const e of list)e.geometry.dispose();this.cache.clear();this.aliases.clear();this.renderer.dispose()}
}
