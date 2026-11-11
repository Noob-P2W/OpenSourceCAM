// Isolated, disposable CAD process: parsing/tessellation never blocks the UI.
self.onmessage = async ({data:{name,bytes}}) => {
 try {
  if(bytes.byteLength>32*1024*1024)throw Error('Model exceeds the 32 MiB import limit');
  const format=/\.(step|stp)$/i.test(name)?'step':/\.(iges|igs)$/i.test(name)?'iges':null;
  if(!format)throw Error('Choose a STEP/STP or IGES/IGS file');
  importScripts('./cad/occt-import-js.js');
  const kernel=await occtimportjs({locateFile:path=>new URL(`./cad/${path}`,self.location.href).href});
  self.postMessage({progress:'Reading CAD geometry and tessellating surfaces…'});
  const params={linearUnit:'millimeter',linearDeflectionType:'absolute_value',linearDeflection:0.1,angularDeflection:0.3};
  const result=format==='step'?kernel.ReadStepFile(new Uint8Array(bytes),params):kernel.ReadIgesFile(new Uint8Array(bytes),params);
  if(!result.success||!result.meshes?.length)throw Error('CAD kernel could not read any surfaces from this file');
  const vertices=[],triangles=[],bodies=[],paths=new Map();
  const pending=result.root?[{node:result.root,path:'',depth:0}]:[];let visited=0;
  while(pending.length){const {node,path,depth}=pending.pop();if(depth>64||++visited>250000)throw Error('CAD assembly hierarchy exceeds supported limits');const label=[path,node.name].filter(Boolean).join(' / ').slice(0,240);for(const index of node.meshes||[])if(!paths.has(index))paths.set(index,label);for(const child of node.children||[])pending.push({node:child,path:label,depth:depth+1})}
  const maxTriangles=250000,maxVertices=750000;
  for(const [sourceIndex,mesh] of result.meshes.entries()){
   const positions=mesh.attributes?.position?.array,indices=mesh.index?.array;
   if(!positions||!indices||positions.length%3||indices.length%3)throw Error('CAD kernel returned invalid mesh data');
   if(triangles.length+indices.length/3>maxTriangles)throw Error(`CAD tessellation has more than ${maxTriangles.toLocaleString('en-US')} triangles at 0.1 mm surface tolerance. No faces were dropped. Split the assembly or use a less detailed source model.`);
   if(!indices.length)continue;
   // Share within each source solid, never across touching assembly components.
   const shared=new Map();
   const path=paths.get(sourceIndex),label=path&&path.split(' / ').at(-1)===mesh.name?path:[path,mesh.name].filter(Boolean).join(' / ');
   bodies.push({name:label.slice(0,240)||`Body ${sourceIndex+1}`,first_triangle:triangles.length,triangle_count:indices.length/3});
   const remap=new Uint32Array(positions.length/3);
   for(let i=0;i<positions.length;i+=3){const point=positions.slice(i,i+3);if(point.some(v=>!Number.isFinite(v)||Math.abs(v)>1000000))throw Error('CAD mesh contains invalid coordinates');const key=point.map(v=>v===0?0:v).join(',');let index=shared.get(key);if(index===undefined){if(vertices.length>=maxVertices)throw Error('CAD mesh exceeds 750,000 unique vertices');index=vertices.length;vertices.push(point);shared.set(key,index)}remap[i/3]=index;}
   for(let i=0;i<indices.length;i+=3){const tri=indices.slice(i,i+3);if(tri.some(n=>!Number.isInteger(n)||n<0||n>=positions.length/3))throw Error('CAD mesh has invalid indices');triangles.push(tri.map(n=>remap[n]))}
  }
  self.postMessage({mesh:{vertices,triangles},bodies,format});
 }catch(error){self.postMessage({error:`CAD import failed: ${error.message||error}`})}
};
