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
 for(const software of [false,true]){
  const pose=await page.evaluate(async software=>{
   const {Viewport}=await import('./viewport.js'),host=document.createElement('div'),canvas=document.createElement('canvas');canvas.style.cssText='width:320px;height:320px';host.append(canvas);document.body.append(host);
   if(software){const get=canvas.getContext.bind(canvas);canvas.getContext=(type,...args)=>type==='webgl'?null:get(type,...args)}
   const viewport=new Viewport(canvas,()=>{},()=>{}),clock=performance.now.bind(performance);let now=1000;performance.now=()=>now;
   try{
    viewport.controls={grid:false,axes:false};viewport.yaw=-Math.PI/2;viewport.elevation=Math.PI/2;viewport.fit('all');
    // Orphan vertices are absent from Rust's compact preview but retained in the raw mesh.
    const raw={vertices:[[1e9,1e9,1e9],[0,0,0],[4,0,0],[4,4,0],[0,4,0],[6,0,0],[10,0,0],[10,4,0],[6,4,0]],triangles:[[1,2,3],[1,3,4],[5,6,7],[5,7,8]]};
    const bodies=[{first_triangle:0,triangle_count:2},{first_triangle:2,triangle_count:2}],matrix=[[0,1,0,0],[-1,0,0,0],[0,0,1,0],[120,-80,0,1]];
    const projectFor=mesh=>({stock:{width:20,height:20,thickness:10},workspace:[{id:1,part:{geometry:{mesh,bodies}}}]});
    const modelFor=(mesh,key)=>({id:1,kind:'vise',vertices:mesh.vertices.slice(1).map(([x,y,z])=>[120-y,-80+x,z]),triangles:mesh.triangles.map(t=>t.map(i=>i-1)),transform:matrix,geometry_key:key,groups_key:'jaw',moving_faces:[2,3]});
    const project=projectFor(raw),model=modelFor(raw,'open'),draw=(p,m)=>viewport.draw(p,[m],null,0,null,null,false,false);
    const pixel=p=>{const [x,y]=viewport.projectWorld(p),px=Math.floor(x),py=Math.floor(y);if(px<0||py<0||px>=canvas.width||py>=canvas.height)return [];if(software)return Array.from(viewport.ctx.getImageData(px,py,1,1).data);const rgb=new Uint8Array(4);viewport.gl.readPixels(px,canvas.height-py-1,1,1,viewport.gl.RGBA,viewport.gl.UNSIGNED_BYTE,rgb);return Array.from(rgb)};
    const solid=p=>{const [r,g,b]=pixel(p);return r>g&&g>b&&r<220};
    // Focus only the reference so a wrong raw-source pose cannot hide inside stock framing.
    viewport.fit({model:1});draw(project,model);
    const placed=solid([118,-78,0])&&solid([118,-72,0]),initialUploads=viewport.three?.uploads;
    const closed={...raw,vertices:raw.vertices.map((p,i)=>i>=5?[p[0]-2,p[1],p[2]]:p)},closedProject=projectFor(closed),closedModel=modelFor(closed,'closed');
    closedModel.jaw_motion={from:[0,2,0],start:null};
    // Include both animation endpoints in the camera frame for this motion check.
    viewport.fit('all');draw(closedProject,closedModel);const startsOpen=solid([118,-78,0])&&solid([118,-72,0]);
    now=1225;draw(closedProject,closedModel);const interpolates=solid([118,-73,0]);
    now=1450;draw(closedProject,closedModel);const endsClosed=solid([118,-78,0])&&solid([118,-74,0])&&!solid([118,-70.5,0]);
    draw(project,model);const reopens=solid([118,-72,0]),reusesShape=software||viewport.three.uploads===initialUploads;
    viewport.fit();draw(project,model);
    const fixed=[118,-78,0],screen=viewport.projectWorld(fixed),scale=viewport.viewScale,stockScreen=viewport.projectWorld([10,10,0]);
    const shifted={...model,vertices:model.vertices.map(p=>[p[0]+6,p[1]+6,p[2]]),transform:matrix.map((column,i)=>i===3?[126,-74,0,1]:column)},shiftedProject={...project,stock:{...project.stock,width:60}};
    viewport.keepPointOnScreen([124,-72,0],screen);draw(shiftedProject,shifted);
    const anchored=viewport.projectWorld([124,-72,0]),newStockScreen=viewport.projectWorld([10,10,0]),ray=viewport.pickRay(...screen),t=-ray.origin[2]/ray.direction[2],picked=ray.origin.map((v,i)=>v+t*ray.direction[i]);
    const screenError=Math.hypot(anchored[0]-screen[0],anchored[1]-screen[1]),scaleError=Math.abs(viewport.viewScale-scale),stockMoved=Math.hypot(newStockScreen[0]-stockScreen[0],newStockScreen[1]-stockScreen[1])>1,pickError=Math.hypot(...picked.map((v,i)=>v-[124,-72,0][i]));
    draw(shiftedProject,shifted);const stable=Math.hypot(...viewport.projectWorld([124,-72,0]).slice(0,2).map((v,i)=>v-screen[i]))<1e-6;
    viewport.magnify(2);draw(shiftedProject,shifted);const zoomWorks=Math.abs(viewport.viewScale-scale*2)<1e-6;
    viewport.fit();draw(shiftedProject,shifted);const fitResets=viewport.fixedScale===null&&viewport.screenAnchor===null&&viewport.zoom===1;
    return{placed,startsOpen,interpolates,endsClosed,reopens,reusesShape,screenError,scaleError,stockMoved,pickError,stable,zoomWorks,fitResets};
   }finally{performance.now=clock;viewport.three?.dispose();host.remove()}
  },software);
  assert.ok(pose.placed,`${software?'Canvas':'WebGL'} must draw rotated/translated reference faces at the preview coordinates`);
  for(const key of ['startsOpen','interpolates','endsClosed','reopens','reusesShape'])assert.ok(pose[key],`${software?'Canvas':'WebGL'} jaw motion/cache: ${key}`);
  assert.ok(pose.screenError<1e-6&&pose.scaleError<1e-6&&pose.pickError<1e-6,`Reference anchor, scale and work-coordinate picking must agree: ${JSON.stringify(pose)}`);
  assert.ok(pose.stockMoved&&pose.stable&&pose.zoomWorks&&pose.fitResets,'Anchoring must move stock on screen, survive redraws, support zoom and reset on Fit');
  console.log(`PASS: ${software?'Canvas':'WebGL'} placed vise pixels, cached close/open jaw poses and fixed-reference screen anchor with work-coordinate picking`);
 }
}finally{await browser.close()}
