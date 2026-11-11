// Presentation only: display coordinates never feed machining calculations.
export class Viewport {
 constructor(canvas,requestDraw,onStatus){
  this.canvas=canvas;this.requestDraw=requestDraw;this.onStatus=onStatus;this.buffers=[];this.zoom=1;this.pan=[0,0];this.yaw=-Math.PI/4;this.elevation=Math.asin(1/Math.sqrt(3));this.pointers=new Map();this.scene=null;this.highlight=null;this.preset='axo';this.controls={dark:false,grid:true,axes:true,rapids:true};this.includeReferences=false;
  this.gl=canvas.getContext('webgl',{alpha:false,antialias:false,powerPreference:'low-power'});
  if(this.gl){try{this.initGL()}catch(error){onStatus(`3D renderer failed: ${error.message}`,true);this.gl=null}}
  if(!this.gl)this.ctx=canvas.getContext('2d');
  canvas.addEventListener('webglcontextlost',event=>{event.preventDefault();this.lost=true;onStatus('3D graphics paused by the browser. Waiting to restore the view…',true)});
  canvas.addEventListener('webglcontextrestored',()=>{this.lost=false;this.buffers=[];this.toolBuffers=[];this.toolSource=null;this.scene=null;this.initGL();requestDraw();onStatus('3D view restored')});
  canvas.addEventListener('pointerdown',event=>{this.pickStart=this.pointers.size===0&&event.button===0?[event.clientX,event.clientY]:null;canvas.setPointerCapture(event.pointerId);this.pointers.set(event.pointerId,[event.clientX,event.clientY]);this.gesture=null;if(this.pointers.size===2){const points=[...this.pointers.values()];this.gesture={distance:Math.hypot(points[0][0]-points[1][0],points[0][1]-points[1][1]),center:[(points[0][0]+points[1][0])/2,(points[0][1]+points[1][1])/2]}}});
  canvas.addEventListener('pointermove',event=>{
   if(!this.pointers.has(event.pointerId))return;
   const old=this.pointers.get(event.pointerId);this.pointers.set(event.pointerId,[event.clientX,event.clientY]);const points=[...this.pointers.values()];
   if(points.length===2){const distance=Math.hypot(points[0][0]-points[1][0],points[0][1]-points[1][1]),center=[(points[0][0]+points[1][0])/2,(points[0][1]+points[1][1])/2];if(this.gesture){this.zoom=Math.max(.1,Math.min(30,this.zoom*distance/Math.max(1,this.gesture.distance)));this.pan[0]+=center[0]-this.gesture.center[0];this.pan[1]+=center[1]-this.gesture.center[1]}this.gesture={distance,center}}
   else if(event.shiftKey||event.buttons===2){this.pan[0]+=event.clientX-old[0];this.pan[1]+=event.clientY-old[1]}
   else {this.yaw-=(event.clientX-old[0])*.012;this.elevation=Math.max(-Math.PI/2,Math.min(Math.PI/2,this.elevation+(event.clientY-old[1])*.012));this.preset='orbit'}
   requestDraw();
  });
  const release=event=>{const start=this.pickStart;this.pickStart=null;this.pointers.delete(event.pointerId);this.gesture=null;if(event.type==='pointerup'&&start&&Math.hypot(event.clientX-start[0],event.clientY-start[1])<5&&this.pickRay){const rect=canvas.getBoundingClientRect();const ray=this.pickRay(event.clientX-rect.left,event.clientY-rect.top);if(this.onPick)this.onPick(ray);else this.onInspect?.(ray)}};
  canvas.addEventListener('pointerup',release);canvas.addEventListener('pointercancel',release);canvas.addEventListener('lostpointercapture',release);canvas.addEventListener('contextmenu',event=>event.preventDefault());
  canvas.addEventListener('wheel',event=>{event.preventDefault();this.magnify(Math.exp(-event.deltaY*.001))},{passive:false});
  canvas.addEventListener('dblclick',()=>this.fit());
 }
 initGL(){
  const gl=this.gl;
  const shader=(type,source)=>{const s=gl.createShader(type);gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));return s};
  const vertex=shader(gl.VERTEX_SHADER,'attribute vec3 position; attribute vec3 normal; uniform mat4 matrix; uniform vec3 offset; varying vec3 n; void main(){gl_Position=matrix*vec4(position+offset,1.0);gl_PointSize=8.0;n=normal;}');
  const fragment=shader(gl.FRAGMENT_SHADER,'precision mediump float; varying vec3 n; uniform vec3 color; uniform float shaded; void main(){float light=1.0;if(shaded>0.5){vec3 normal=normalize(n);if(!gl_FrontFacing)normal=-normal;light=0.4+0.6*max(0.0,dot(normal,normalize(vec3(0.3,-0.5,0.8))));}gl_FragColor=vec4(color*light,1.0);}');
  this.program=gl.createProgram();gl.attachShader(this.program,vertex);gl.attachShader(this.program,fragment);gl.linkProgram(this.program);gl.deleteShader(vertex);gl.deleteShader(fragment);if(!gl.getProgramParameter(this.program,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(this.program));
  this.markerBuffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,this.markerBuffer);gl.bufferData(gl.ARRAY_BUFFER,new Float32Array(6),gl.DYNAMIC_DRAW);this.locations={};for(const name of ['matrix','offset','color','shaded'])this.locations[name]=gl.getUniformLocation(this.program,name);for(const name of ['position','normal'])this.locations[name]=gl.getAttribLocation(this.program,name);
 }
 setView(preset){this.preset=preset;this.yaw=preset==='top'||preset==='front'?-Math.PI/2:preset==='right'?0:-Math.PI/4;this.elevation=preset==='top'?Math.PI/2:['front','right'].includes(preset)?0:Math.asin(1/Math.sqrt(3));this.fit(this.includeReferences?'all':'part')}
 fit(mode='part'){this.includeReferences=mode==='all';this.zoom=1;this.pan=[0,0];this.requestDraw()}
 magnify(factor){this.zoom=Math.max(.1,Math.min(30,this.zoom*factor));this.requestDraw()}
 upload(data,color,lines=false,retain=true){const item={data,color,lines,count:data.length/6};if(this.gl){item.buffer=this.gl.createBuffer();this.gl.bindBuffer(this.gl.ARRAY_BUFFER,item.buffer);this.gl.bufferData(this.gl.ARRAY_BUFFER,data,this.gl.STATIC_DRAW)}if(retain)this.buffers.push(item);return item}
 mesh(mesh,transform,color,retain=true){
  const data=new Float32Array(mesh.triangles.length*18);let offset=0;
  for(const triangle of mesh.triangles){const p=triangle.map(i=>transform(mesh.vertices[i]));const u=p[1].map((v,i)=>v-p[0][i]),v=p[2].map((v,i)=>v-p[0][i]);let n=[u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]];const length=Math.hypot(...n)||1;n=n.map(x=>x/length);for(const vertex of p){data.set(vertex,offset);data.set(n,offset+3);offset+=6}}
  return this.upload(data,color,false,retain);
 }
 setTool(tool){
  if(this.toolSource===tool)return;
  for(const item of this.toolBuffers||[])if(item.buffer)this.gl?.deleteBuffer(item.buffer);
  this.toolSource=tool;
  this.toolBuffers=tool?tool.meshes.map((mesh,index)=>this.mesh(mesh,v=>v,index?[.95,.65,.18]:[.66,.75,.82],false)):[];
 }
 lines(points,color){const data=new Float32Array(points.length*6);points.forEach((p,i)=>data.set(p,i*6));return this.upload(data,color,true)}
 rebuild(project,workspace,simulation){
  this.canvas.dataset.sceneBuilds=String(Number(this.canvas.dataset.sceneBuilds||0)+1);
  for(const item of this.buffers)if(item.buffer){this.gl?.deleteBuffer(item.buffer);if(item.wireBuffer)this.gl?.deleteBuffer(item.wireBuffer)}this.buffers=[];this.scene=project;this.simulation=simulation;this.pathSource=null;this.pathFilter=undefined;this.pathBounds=null;this.pathBuffers=[];this.pathPrefixes=[];this.highlightSource=null;this.highlightBuffer=null;this.stockPieces=new Map();this.stockBuffers=[];
  const stock=project.stock;this.bounds={min:[0,0,-stock.thickness],max:[stock.width,stock.height,0]};
  const include=p=>{for(let i=0;i<3;i++){this.bounds.min[i]=Math.min(this.bounds.min[i],p[i]);this.bounds.max[i]=Math.max(this.bounds.max[i],p[i])}};
  if(project.part?.visible){const part=project.part,f=part.units==='inches'?25.4:1;const matrix=project._display?.part_transform;const transform=v=>matrix?[0,1,2].map(i=>matrix[0][i]*v[0]+matrix[1][i]*v[1]+matrix[2][i]*v[2]+matrix[3][i]):v.map((x,i)=>x*f+part.offset[i]);for(const v of part.geometry.mesh.vertices)include(transform(v));if(!simulation)this.mesh(part.geometry.mesh,transform,[.6,.79,.88])}
  if(!simulation&&this.controls.clampingStock&&project._display?.clamping_stock)this.mesh(project._display.clamping_stock,v=>v,[.9,.69,.3]).setupStock=true;
  this.allBounds={min:[...this.bounds.min],max:[...this.bounds.max]};for(const model of workspace){for(const p of model.vertices)for(let i=0;i<3;i++){this.allBounds.min[i]=Math.min(this.allBounds.min[i],p[i]);this.allBounds.max[i]=Math.max(this.allBounds.max[i],p[i])}if(model.jaw_motion)for(const index of model.moving_faces||[])for(const vertex of model.triangles[index])for(let i=0;i<3;i++){const value=model.vertices[vertex][i]+model.jaw_motion.from[i];this.allBounds.min[i]=Math.min(this.allBounds.min[i],value);this.allBounds.max[i]=Math.max(this.allBounds.max[i],value)}const moving=new Set(model.moving_faces||[]),color=model.kind==='machine'?[.5,.55,.61]:[.7,.65,.52];for(const selected of moving.size?[false,true]:[false]){const mesh={vertices:model.vertices,triangles:model.triangles.filter((_,i)=>moving.has(i)===selected)};const item=this.mesh(mesh,v=>v,color);item.reference=true;if(selected)item.jawMotion=model.jaw_motion}}
  this.triangleCount=this.buffers.reduce((n,b)=>n+b.count/3,0);
  const stockStart=this.buffers.length;
  if(simulation?.mesh)this.mesh(simulation.mesh,v=>v,[.45,.7,.4]);
  else if(simulation?.heights){const [nx,ny]=simulation.summary.resolution,[dx,dy]=simulation.summary.cell_size;const mesh={vertices:[],triangles:[]};for(let y=0;y<ny;y++)for(let x=0;x<nx;x++){const z=simulation.heights[y*nx+x],i=mesh.vertices.length;mesh.vertices.push([x*dx,y*dy,z],[(x+1)*dx,y*dy,z],[(x+1)*dx,(y+1)*dy,z],[x*dx,(y+1)*dy,z]);mesh.triangles.push([i,i+1,i+2],[i,i+2,i+3])}this.mesh(mesh,v=>v,[.45,.7,.4])}
  this.stockBuffers=this.buffers.slice(stockStart);
  if(simulation?.patches)this.updateStockPatches(simulation,true);else{this.stockCpu=null;this.stockFrameSource=null}
  const corners=[];for(const z of [-stock.thickness,0])corners.push([0,0,z],[stock.width,0,z],[stock.width,stock.height,z],[0,stock.height,z]);const segments=[];for(let i=0;i<4;i++)segments.push(corners[i],corners[(i+1)%4],corners[i+4],corners[(i+1)%4+4],corners[i],corners[i+4]);this.stockLines=this.lines(project._display?.stock_outline||segments,this.controls.dark?[.9,.7,.3]:[.65,.42,.1]);
  const axis=Math.max(.001,Math.min(stock.width,stock.height,stock.thickness)*.5);this.axisBuffers=[this.lines([[0,0,0],[axis,0,0]],[.88,.22,.22]),this.lines([[0,0,0],[0,axis,0]],[.2,.65,.3]),this.lines([[0,0,0],[0,0,axis]],[.2,.42,.9])];const grid=[],extent=Math.max(stock.width,stock.height),step=10**Math.floor(Math.log10(extent/8)),cx=stock.width/2,cy=stock.height/2,z=this.allBounds.min[2]-.01;for(let i=-10;i<=10;i++){grid.push([cx+i*step,cy-10*step,z],[cx+i*step,cy+10*step,z],[cx-10*step,cy+i*step,z],[cx+10*step,cy+i*step,z])}this.gridLines=this.lines(grid,this.controls.dark?[.16,.21,.26]:[.83,.85,.86]);
 }
 updateStockPatches(frame,rebuild=false){
  const remove=item=>{if(item.buffer)this.gl?.deleteBuffer(item.buffer);if(item.wireBuffer)this.gl?.deleteBuffer(item.wireBuffer);const i=this.buffers.indexOf(item);if(i>=0)this.buffers.splice(i,1)};
  const sameFrame=this.stockFrameSource===frame,reset=!this.stockCpu||this.stockFrameEpoch!==frame.epoch||this.stockFrameResolution!==frame.resolution||(frame.reset&&!sameFrame);
  if(reset)this.stockCpu=new Map();
  if(!sameFrame)for(const patch of frame.patches)this.stockCpu.set(patch.strip,frame.vertices.slice(patch.offset,patch.offset+patch.length));
  this.stockFrameSource=frame;this.stockFrameEpoch=frame.epoch;this.stockFrameResolution=frame.resolution;
  const all=reset||rebuild||!this.stockPieces.size;if(all){for(const item of this.stockBuffers)remove(item);this.stockBuffers=[];this.stockPieces.clear()}
  const strips=all?this.stockCpu.keys():frame.patches.map(p=>p.strip);
  for(const strip of strips){const old=this.stockPieces.get(strip);if(old){remove(old);this.stockBuffers.splice(this.stockBuffers.indexOf(old),1)}const item=this.upload(this.stockCpu.get(strip),[.45,.7,.4]);this.stockPieces.set(strip,item);this.stockBuffers.push(item)}
  this.canvas.dataset.stockTriangles=String(this.stockBuffers.reduce((n,item)=>n+item.count/3,0));
 }
 updatePath(path,filter){
  const visibility=[...(this.controls.hiddenOperations||[])].sort().join(',');if(this.pathSource===path&&this.pathFilter===filter&&this.pathVisibility===visibility)return;this.pathVisibility=visibility;if(this.toolPath!==path){this.toolEnvelope=null;this.toolPath=path}this.pathSource=path;this.pathFilter=filter;
  for(const item of this.pathBuffers||[]){if(item.buffer)this.gl?.deleteBuffer(item.buffer);this.buffers.splice(this.buffers.indexOf(item),1)}this.pathBuffers=[];this.pathBounds=null;
  if(!path)return;this.pathBounds={min:[Infinity,Infinity,Infinity],max:[-Infinity,-Infinity,-Infinity]};const cuts=[],rapids=[];this.pathPrefixes=[new Uint32Array(path.moves.length+1),new Uint32Array(path.moves.length+1)];path.moves.forEach((move,index)=>{for(const p of [move.from,move.to])for(let i=0;i<3;i++){this.pathBounds.min[i]=Math.min(this.pathBounds.min[i],p[i]);this.pathBounds.max[i]=Math.max(this.pathBounds.max[i],p[i])}const included=(filter===null||move.operation===filter)&&!this.controls.hiddenOperations?.has(move.operation);if(included)(move.rapid?rapids:cuts).push(move.from,move.to);for(let i=0;i<2;i++)this.pathPrefixes[i][index+1]=this.pathPrefixes[i][index]+(included&&Number(move.rapid)===i?2:0)});
  this.pathBuffers=[this.lines(cuts,this.controls.dark?[.33,.96,.77]:[.02,.5,.4]),this.lines(rapids,[.76,.48,.28])];
 }
 draw(project,workspace,path,progress,simulation,highlight,wireframe,showStock,filter=null,toolForOperation=null){
  const canvas=this.canvas,w=canvas.clientWidth,h=canvas.clientHeight;if(!w||!h||this.lost)return;
  const dpr=Math.min(devicePixelRatio||1,2,Math.sqrt(4000000/(w*h)));if(canvas.width!==Math.round(w*dpr))canvas.width=Math.round(w*dpr);if(canvas.height!==Math.round(h*dpr))canvas.height=Math.round(h*dpr);
  if(this.scene!==project||Boolean(this.simulation)!==Boolean(simulation)||(this.simulation!==simulation&&!simulation?.patches))this.rebuild(project,workspace,simulation);else if(this.simulation!==simulation){this.updateStockPatches(simulation);this.simulation=simulation}this.updatePath(path,filter);
  if(this.highlightSource!==highlight){
   for(const item of this.highlightItems||[]){if(item.buffer)this.gl?.deleteBuffer(item.buffer);const i=this.buffers.indexOf(item);if(i>=0)this.buffers.splice(i,1)}
   this.highlightItems=[];this.highlightSource=highlight;
   if(highlight){const triangles=highlight.fill||((highlight.outline||[]).slice(1,-1).map((p,i)=>[highlight.outline[0],p,highlight.outline[i+2]]));if(triangles.length){const vertices=triangles.flat(),mesh={vertices,triangles:triangles.map((_,i)=>[i*3,i*3+1,i*3+2])};const fill=this.mesh(mesh,v=>v,[1,.7,.28]);fill.selection=true;this.highlightItems.push(fill)}this.highlightItems.push(this.lines(highlight.segments||highlight.outline.flatMap((p,i,a)=>[p,a[(i+1)%a.length]]),[1,.45,.04]));}
  }

  // Both the model's tip and the old marker use the shared estimated-time cursor.
  const time=path?Math.max(0,Math.min(100,progress))/100*path.seconds:0;
  let completed=0;
  if(path){let lo=0,hi=path.moves.length;while(lo<hi){const mid=(lo+hi)>>>1;if(path.moves[mid].end_seconds<=time)lo=mid+1;else hi=mid}completed=lo}
  const move=path?.moves[Math.min(completed,path.moves.length-1)];
  const fraction=move?(completed===path.moves.length?1:Math.max(0,Math.min(1,(time-move.start_seconds)/(move.end_seconds-move.start_seconds)))):0;
  const cursor=move?move.from.map((v,i)=>v+(move.to[i]-v)*fraction):null;
  const showCursor=cursor&&(filter===null||move.operation===filter);
  const tool=showCursor?toolForOperation?.(move.operation):null;this.setTool(tool||null);
  if(tool){if(!this.toolEnvelope)this.toolEnvelope={min:[...tool.min],max:[...tool.max]};else for(let i=0;i<3;i++){this.toolEnvelope.min[i]=Math.min(this.toolEnvelope.min[i],tool.min[i]);this.toolEnvelope.max[i]=Math.max(this.toolEnvelope.max[i],tool.max[i])}}
  // Fit the tool's swept envelope, rather than moving the camera with the cutter.
  const frame=this.includeReferences?this.allBounds:this.bounds;const bounds={min:frame.min.map((v,i)=>Math.min(v,(this.pathBounds?.min[i]??v)+(this.toolEnvelope?.min[i]??0))),max:frame.max.map((v,i)=>Math.max(v,(this.pathBounds?.max[i]??v)+(this.toolEnvelope?.max[i]??0)))};
  for(const p of this.annotationPoints||[])for(let i=0;i<3;i++){bounds.min[i]=Math.min(bounds.min[i],p[i]);bounds.max[i]=Math.max(bounds.max[i],p[i])}
  const center=bounds.min.map((v,i)=>(v+bounds.max[i])/2),radius=Math.max(.001,Math.hypot(...bounds.max.map((v,i)=>v-bounds.min[i]))/2);
  const cy=Math.cos(this.yaw),sy=Math.sin(this.yaw),ce=Math.cos(this.elevation),se=Math.sin(this.elevation);const right=[-sy,cy,0],up=[-se*cy,-se*sy,ce],depth=[ce*cy,ce*sy,se];const dot=(a,b)=>a.reduce((n,x,i)=>n+x*b[i],0);
  let ex=0,ey=0;for(let mask=0;mask<8;mask++){const v=center.map((x,i)=>(mask&(1<<i)?bounds.max[i]:bounds.min[i])-x);ex=Math.max(ex,Math.abs(dot(v,right)));ey=Math.max(ey,Math.abs(dot(v,up)))}const margin=Math.min(this.annotationPoints?.length?100:40,Math.min(w,h)*0.4);const scale=Math.min((w-margin)/(2*Math.max(.001,ex)),(h-margin)/(2*Math.max(.001,ey)))*this.zoom;
  this.pickRay=(x,y)=>{const u=(x-w/2-this.pan[0])/scale,v=(h/2+this.pan[1]-y)/scale;return {origin:center.map((c,i)=>c+u*right[i]+v*up[i]+depth[i]*radius*4),direction:depth.map(c=>-c)}};
  const projectPoint=p=>{const v=p.map((x,i)=>x-center[i]);return [w/2+this.pan[0]+dot(v,right)*scale,h/2+this.pan[1]-dot(v,up)*scale,dot(v,depth)]};
  const jawOffset=item=>{if(!item.jawMotion)return [0,0,0];if(item.jawMotion.start===null)item.jawMotion.start=performance.now();const fraction=Math.max(0,1-(performance.now()-item.jawMotion.start)/450);if(fraction>0)this.requestDraw();return item.jawMotion.from.map(v=>v*fraction)};
  const visible=this.buffers.filter(item=>(!item.setupStock||showStock)&&(item!==this.stockLines||showStock)&&(item!==this.gridLines||this.controls.grid)&&(!this.axisBuffers.includes(item)||this.controls.axes)&&(!this.pathBuffers.includes(item)||this.controls.paths!==false)&&(item!==this.pathBuffers[1]||this.controls.rapids)).sort((a,b)=>(a.lines?2:a.selection?1:0)-(b.lines?2:b.selection?1:0));
  if(this.gl){
   const gl=this.gl;gl.viewport(0,0,canvas.width,canvas.height);gl.clearColor(...(this.controls.dark?[.055,.09,.12,1]:[.95,.96,.965,1]));gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);gl.enable(gl.DEPTH_TEST);gl.depthFunc(gl.LEQUAL);gl.useProgram(this.program);
   const sx=2*scale/w,sy2=2*scale/h,sz=-1/(radius*4);const matrix=new Float32Array([right[0]*sx,up[0]*sy2,depth[0]*sz,0,right[1]*sx,up[1]*sy2,depth[1]*sz,0,right[2]*sx,up[2]*sy2,depth[2]*sz,0,-dot(center,right)*sx+this.pan[0]*2/w,-dot(center,up)*sy2-this.pan[1]*2/h,-dot(center,depth)*sz,1]);gl.uniformMatrix4fv(this.locations.matrix,false,matrix);gl.uniform3fv(this.locations.offset,[0,0,0]);
   for(const item of visible){
    gl.uniform3fv(this.locations.offset,jawOffset(item));let drawBuffer=item.buffer,drawCount=item.count,mode=item.lines?gl.LINES:gl.TRIANGLES;
    if(wireframe&&!item.lines){if(!item.wireBuffer){const stride=Math.max(1,Math.ceil(item.count/3/12000)),edges=[];for(let i=0;i<item.count;i+=3*stride)for(const j of [0,1,1,2,2,0])edges.push(...item.data.subarray((i+j)*6,(i+j)*6+6));const wireData=new Float32Array(edges);item.wireBuffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,item.wireBuffer);gl.bufferData(gl.ARRAY_BUFFER,wireData,gl.STATIC_DRAW);item.wireCount=wireData.length/6}drawBuffer=item.wireBuffer;drawCount=item.wireCount;mode=gl.LINES}
    gl.bindBuffer(gl.ARRAY_BUFFER,drawBuffer);gl.enableVertexAttribArray(this.locations.position);gl.enableVertexAttribArray(this.locations.normal);gl.vertexAttribPointer(this.locations.position,3,gl.FLOAT,false,24,0);gl.vertexAttribPointer(this.locations.normal,3,gl.FLOAT,false,24,12);gl.uniform3fv(this.locations.color,item.color);gl.uniform1f(this.locations.shaded,item.lines||wireframe?0:1);let count=drawCount;const index=this.pathBuffers?.indexOf(item);if(index>=0)count=this.pathPrefixes[index][completed];if(item.selection){gl.enable(gl.POLYGON_OFFSET_FILL);gl.polygonOffset(-2,-2)}gl.drawArrays(mode,0,count);if(item.selection)gl.disable(gl.POLYGON_OFFSET_FILL)}
   if(tool){gl.uniform3fv(this.locations.offset,cursor);for(const item of this.toolBuffers){gl.bindBuffer(gl.ARRAY_BUFFER,item.buffer);gl.vertexAttribPointer(this.locations.position,3,gl.FLOAT,false,24,0);gl.vertexAttribPointer(this.locations.normal,3,gl.FLOAT,false,24,12);gl.uniform3fv(this.locations.color,item.color);gl.uniform1f(this.locations.shaded,1);gl.drawArrays(gl.TRIANGLES,0,item.count)}}
   else if(showCursor){gl.bindBuffer(gl.ARRAY_BUFFER,this.markerBuffer);gl.bufferSubData(gl.ARRAY_BUFFER,0,new Float32Array([...cursor,0,0,0]));gl.vertexAttribPointer(this.locations.position,3,gl.FLOAT,false,24,0);gl.vertexAttribPointer(this.locations.normal,3,gl.FLOAT,false,24,12);gl.uniform3fv(this.locations.color,[1,.35,.08]);gl.uniform1f(this.locations.shaded,0);gl.disable(gl.DEPTH_TEST);gl.drawArrays(gl.POINTS,0,1)}
   canvas.dataset.renderer='webgl';
  }else if(this.ctx){
   const ctx=this.ctx;ctx.setTransform(dpr,0,0,dpr,0,0);ctx.fillStyle=this.controls.dark?'#0e171f':'#f2f5f6';ctx.fillRect(0,0,w,h);const faces=[];
   const drawLine=item=>{const line=new Path2D();let count=item.count;const index=this.pathBuffers?.indexOf(item);if(index>=0)count=this.pathPrefixes[index][completed];for(let i=0;i<count;i+=2){const a=projectPoint(Array.from(item.data.subarray(i*6,i*6+3))),b=projectPoint(Array.from(item.data.subarray((i+1)*6,(i+1)*6+3)));line.moveTo(a[0],a[1]);line.lineTo(b[0],b[1])}ctx.strokeStyle=`rgb(${item.color.map(v=>Math.round(v*255)).join(',')})`;ctx.stroke(line)};
   if(visible.includes(this.gridLines))drawLine(this.gridLines);
   for(const item of visible){if(item.lines)continue;const motion=jawOffset(item);const stride=item.reference?1:Math.max(1,Math.ceil(item.count/3/Math.max(100,12000/visible.length)));for(let i=0;i<item.count;i+=3*stride){const p=[0,1,2].map(j=>projectPoint(Array.from(item.data.subarray((i+j)*6,(i+j)*6+3),(v,k)=>v+motion[k])));faces.push({p,color:item.color,reference:item.reference,z:p.reduce((n,v)=>n+v[2],0)/3})}}
   if(tool)for(const item of this.toolBuffers)for(let i=0;i<item.count;i+=3){const p=[0,1,2].map(j=>projectPoint(Array.from(item.data.subarray((i+j)*6,(i+j)*6+3),(v,k)=>v+cursor[k])));faces.push({p,color:item.color,z:p.reduce((n,v)=>n+v[2],0)/3,tool:true})}
   faces.sort((a,b)=>a.z-b.z);for(const face of faces){ctx.beginPath();face.p.forEach((v,i)=>i?ctx.lineTo(v[0],v[1]):ctx.moveTo(v[0],v[1]));ctx.closePath();ctx.fillStyle=`rgb(${face.color.map(v=>Math.round(v*200)).join(',')})`;if(wireframe&&!face.tool)ctx.stroke();else{ctx.fill();if(face.reference){ctx.strokeStyle=ctx.fillStyle;ctx.lineJoin='round';ctx.lineWidth=1;ctx.stroke();ctx.lineJoin='miter'}}}
   for(const item of visible.filter(x=>x.lines&&x!==this.gridLines))drawLine(item);
   if(showCursor&&!tool){const p=projectPoint(cursor);ctx.fillStyle='#e65100';ctx.beginPath();ctx.arc(p[0],p[1],4,0,Math.PI*2);ctx.fill()}
   canvas.dataset.renderer='canvas';
  }else this.onStatus('Graphics are unavailable in this browser. Enable graphics acceleration and reload.',true);
  canvas.dataset.referenceTriangles=String(this.buffers.filter(b=>b.reference).reduce((n,b)=>n+b.count/3,0));canvas.dataset.displayProgress=String(progress);canvas.dataset.cuttingDiameter=tool?String(tool.cutting_diameter):'';canvas.dataset.triangles=String(this.triangleCount);canvas.dataset.zoom=String(this.zoom);canvas.dataset.yaw=String(this.yaw);
  canvas.dataset.toolTriangles=String((this.toolBuffers||[]).reduce((n,item)=>n+item.count/3,0));canvas.dataset.toolId=tool?String(tool.library_id??'legacy'):'';canvas.dataset.toolTip=showCursor?cursor.join(','):'';canvas.dataset.toolOperation=showCursor?String(move.operation):'';
  return {triangles:this.triangleCount,fallback:!this.gl,projectPoint,camera:depth,operation:showCursor?move.operation:null,tool};
 }
}
