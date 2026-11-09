// Presentation only: display coordinates never feed machining calculations.
export class Viewport {
 constructor(canvas,requestDraw,onStatus){
  this.canvas=canvas;this.requestDraw=requestDraw;this.onStatus=onStatus;this.buffers=[];this.zoom=1;this.pan=[0,0];this.yaw=-Math.PI/4;this.elevation=.55;this.pointers=new Map();this.scene=null;this.highlight=null;this.preset='iso';
  this.gl=canvas.getContext('webgl',{alpha:false,antialias:false,powerPreference:'low-power'});
  if(this.gl){try{this.initGL()}catch(error){onStatus(`3D renderer failed: ${error.message}`,true);this.gl=null}}
  if(!this.gl)this.ctx=canvas.getContext('2d');
  canvas.addEventListener('webglcontextlost',event=>{event.preventDefault();this.lost=true;onStatus('3D graphics paused by the browser. Waiting to restore the view…',true)});
  canvas.addEventListener('webglcontextrestored',()=>{this.lost=false;this.buffers=[];this.scene=null;this.initGL();requestDraw();onStatus('3D view restored')});
  canvas.addEventListener('pointerdown',event=>{canvas.setPointerCapture(event.pointerId);this.pointers.set(event.pointerId,[event.clientX,event.clientY]);this.gesture=null;if(this.pointers.size===2){const points=[...this.pointers.values()];this.gesture={distance:Math.hypot(points[0][0]-points[1][0],points[0][1]-points[1][1]),center:[(points[0][0]+points[1][0])/2,(points[0][1]+points[1][1])/2]}}});
  canvas.addEventListener('pointermove',event=>{
   if(!this.pointers.has(event.pointerId))return;
   const old=this.pointers.get(event.pointerId);this.pointers.set(event.pointerId,[event.clientX,event.clientY]);const points=[...this.pointers.values()];
   if(points.length===2){const distance=Math.hypot(points[0][0]-points[1][0],points[0][1]-points[1][1]),center=[(points[0][0]+points[1][0])/2,(points[0][1]+points[1][1])/2];if(this.gesture){this.zoom=Math.max(.1,Math.min(30,this.zoom*distance/Math.max(1,this.gesture.distance)));this.pan[0]+=center[0]-this.gesture.center[0];this.pan[1]+=center[1]-this.gesture.center[1]}this.gesture={distance,center}}
   else if(event.shiftKey||event.buttons===2){this.pan[0]+=event.clientX-old[0];this.pan[1]+=event.clientY-old[1]}
   else {this.yaw-=(event.clientX-old[0])*.012;this.elevation=Math.max(-Math.PI/2,Math.min(Math.PI/2,this.elevation+(event.clientY-old[1])*.012));this.preset='orbit'}
   requestDraw();
  });
  const release=event=>{this.pointers.delete(event.pointerId);this.gesture=null};canvas.addEventListener('pointerup',release);canvas.addEventListener('pointercancel',release);canvas.addEventListener('lostpointercapture',release);canvas.addEventListener('contextmenu',event=>event.preventDefault());
  canvas.addEventListener('wheel',event=>{event.preventDefault();this.magnify(Math.exp(-event.deltaY*.001))},{passive:false});
  canvas.addEventListener('dblclick',()=>this.fit());
 }
 initGL(){
  const gl=this.gl;
  const shader=(type,source)=>{const s=gl.createShader(type);gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));return s};
  const vertex=shader(gl.VERTEX_SHADER,'attribute vec3 position; attribute vec3 normal; uniform mat4 matrix; varying vec3 n; void main(){gl_Position=matrix*vec4(position,1.0);gl_PointSize=8.0;n=normal;}');
  const fragment=shader(gl.FRAGMENT_SHADER,'precision mediump float; varying vec3 n; uniform vec3 color; uniform float shaded; void main(){float light=1.0;if(shaded>0.5){vec3 normal=normalize(n);if(!gl_FrontFacing)normal=-normal;light=0.4+0.6*max(0.0,dot(normal,normalize(vec3(0.3,-0.5,0.8))));}gl_FragColor=vec4(color*light,1.0);}');
  this.program=gl.createProgram();gl.attachShader(this.program,vertex);gl.attachShader(this.program,fragment);gl.linkProgram(this.program);gl.deleteShader(vertex);gl.deleteShader(fragment);if(!gl.getProgramParameter(this.program,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(this.program));
  this.markerBuffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,this.markerBuffer);gl.bufferData(gl.ARRAY_BUFFER,new Float32Array(6),gl.DYNAMIC_DRAW);this.locations={};for(const name of ['matrix','color','shaded'])this.locations[name]=gl.getUniformLocation(this.program,name);for(const name of ['position','normal'])this.locations[name]=gl.getAttribLocation(this.program,name);
 }
 setView(preset){this.preset=preset;this.yaw=preset==='top'?-Math.PI/2:preset==='front'?-Math.PI/2:-Math.PI/4;this.elevation=preset==='top'?Math.PI/2:preset==='front'?0:.55;this.fit()}
 fit(){this.zoom=1;this.pan=[0,0];this.requestDraw()}
 magnify(factor){this.zoom=Math.max(.1,Math.min(30,this.zoom*factor));this.requestDraw()}
 upload(data,color,lines=false){const item={data,color,lines,count:data.length/6};if(this.gl){item.buffer=this.gl.createBuffer();this.gl.bindBuffer(this.gl.ARRAY_BUFFER,item.buffer);this.gl.bufferData(this.gl.ARRAY_BUFFER,data,this.gl.STATIC_DRAW)}this.buffers.push(item);return item}
 mesh(mesh,transform,color){
  const data=new Float32Array(mesh.triangles.length*18);let offset=0;
  for(const triangle of mesh.triangles){const p=triangle.map(i=>transform(mesh.vertices[i]));const u=p[1].map((v,i)=>v-p[0][i]),v=p[2].map((v,i)=>v-p[0][i]);let n=[u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]];const length=Math.hypot(...n)||1;n=n.map(x=>x/length);for(const vertex of p){data.set(vertex,offset);data.set(n,offset+3);offset+=6}}
  return this.upload(data,color);
 }
 lines(points,color){const data=new Float32Array(points.length*6);points.forEach((p,i)=>data.set(p,i*6));return this.upload(data,color,true)}
 rebuild(project,workspace,simulation){
  for(const item of this.buffers)if(item.buffer){this.gl?.deleteBuffer(item.buffer);if(item.wireBuffer)this.gl?.deleteBuffer(item.wireBuffer)}this.buffers=[];this.scene=project;this.simulation=simulation;this.pathSource=null;this.pathFilter=undefined;this.pathBounds=null;this.pathBuffers=[];this.pathPrefixes=[];this.highlightSource=null;this.highlightBuffer=null;
  const stock=project.stock;this.bounds={min:[0,0,-stock.thickness],max:[stock.width,stock.height,0]};
  const include=p=>{for(let i=0;i<3;i++){this.bounds.min[i]=Math.min(this.bounds.min[i],p[i]);this.bounds.max[i]=Math.max(this.bounds.max[i],p[i])}};
  if(project.part?.visible){const part=project.part,f=part.units==='inches'?25.4:1;const transform=v=>v.map((x,i)=>x*f+part.offset[i]);for(const v of part.geometry.mesh.vertices)include(transform(v));this.mesh(part.geometry.mesh,transform,[.6,.79,.88])}
  for(const model of workspace)this.mesh(model,v=>v,model.kind==='machine'?[.35,.43,.53]:[.72,.56,.3]);
  this.triangleCount=this.buffers.reduce((n,b)=>n+b.count/3,0);
  if(simulation?.mesh)this.mesh(simulation.mesh,v=>v,[.45,.7,.4]);
  else if(simulation){const [nx,ny]=simulation.summary.resolution,[dx,dy]=simulation.summary.cell_size;const mesh={vertices:[],triangles:[]};for(let y=0;y<ny;y++)for(let x=0;x<nx;x++){const z=simulation.heights[y*nx+x],i=mesh.vertices.length;mesh.vertices.push([x*dx,y*dy,z],[(x+1)*dx,y*dy,z],[(x+1)*dx,(y+1)*dy,z],[x*dx,(y+1)*dy,z]);mesh.triangles.push([i,i+1,i+2],[i,i+2,i+3])}this.mesh(mesh,v=>v,[.45,.7,.4])}
  const corners=[];for(const z of [-stock.thickness,0])corners.push([0,0,z],[stock.width,0,z],[stock.width,stock.height,z],[0,stock.height,z]);const segments=[];for(let i=0;i<4;i++)segments.push(corners[i],corners[(i+1)%4],corners[i+4],corners[(i+1)%4+4],corners[i],corners[i+4]);this.stockLines=this.lines(segments,[.86,.67,.32]);
  const axis=Math.max(.001,Math.min(stock.width,stock.height,stock.thickness)*.5);this.lines([[0,0,0],[axis,0,0]],[1,.3,.3]);this.lines([[0,0,0],[0,axis,0]],[.3,1,.4]);this.lines([[0,0,0],[0,0,axis]],[.4,.6,1]);
 }
 updatePath(path,filter){
  if(this.pathSource===path&&this.pathFilter===filter)return;this.pathSource=path;this.pathFilter=filter;
  for(const item of this.pathBuffers||[]){if(item.buffer)this.gl?.deleteBuffer(item.buffer);this.buffers.splice(this.buffers.indexOf(item),1)}this.pathBuffers=[];this.pathBounds=null;
  if(!path)return;this.pathBounds={min:[Infinity,Infinity,Infinity],max:[-Infinity,-Infinity,-Infinity]};const cuts=[],rapids=[];this.pathPrefixes=[new Uint32Array(path.moves.length+1),new Uint32Array(path.moves.length+1)];path.moves.forEach((move,index)=>{for(const p of [move.from,move.to])for(let i=0;i<3;i++){this.pathBounds.min[i]=Math.min(this.pathBounds.min[i],p[i]);this.pathBounds.max[i]=Math.max(this.pathBounds.max[i],p[i])}const included=filter===null||move.operation===filter;if(included)(move.rapid?rapids:cuts).push(move.from,move.to);for(let i=0;i<2;i++)this.pathPrefixes[i][index+1]=this.pathPrefixes[i][index]+(included&&Number(move.rapid)===i?2:0)});
  this.pathBuffers=[this.lines(cuts,[.33,.96,.77]),this.lines(rapids,[.76,.48,.28])];
 }
 draw(project,workspace,path,progress,simulation,highlight,wireframe,showStock,filter=null){
  const canvas=this.canvas,w=canvas.clientWidth,h=canvas.clientHeight;if(!w||!h||this.lost)return;
  const dpr=Math.min(devicePixelRatio||1,2,Math.sqrt(4000000/(w*h)));if(canvas.width!==Math.round(w*dpr))canvas.width=Math.round(w*dpr);if(canvas.height!==Math.round(h*dpr))canvas.height=Math.round(h*dpr);
  if(this.scene!==project||this.simulation!==simulation)this.rebuild(project,workspace,simulation);this.updatePath(path,filter);
  if(this.highlightSource!==highlight){if(this.highlightBuffer){if(this.highlightBuffer.buffer)this.gl?.deleteBuffer(this.highlightBuffer.buffer);this.buffers.splice(this.buffers.indexOf(this.highlightBuffer),1)}this.highlightSource=highlight;this.highlightBuffer=highlight?this.lines(highlight.outline.flatMap((p,i,a)=>[p,a[(i+1)%a.length]]),[1,.6,.1]):null}
  const bounds={min:this.bounds.min.map((v,i)=>Math.min(v,this.pathBounds?.min[i]??v)),max:this.bounds.max.map((v,i)=>Math.max(v,this.pathBounds?.max[i]??v))};
  const center=bounds.min.map((v,i)=>(v+bounds.max[i])/2),radius=Math.max(.001,Math.hypot(...bounds.max.map((v,i)=>v-bounds.min[i]))/2);
  const cy=Math.cos(this.yaw),sy=Math.sin(this.yaw),ce=Math.cos(this.elevation),se=Math.sin(this.elevation);const right=[-sy,cy,0],up=[-se*cy,-se*sy,ce],depth=[ce*cy,ce*sy,se];const dot=(a,b)=>a.reduce((n,x,i)=>n+x*b[i],0);
  let ex=0,ey=0;for(let mask=0;mask<8;mask++){const v=center.map((x,i)=>(mask&(1<<i)?bounds.max[i]:bounds.min[i])-x);ex=Math.max(ex,Math.abs(dot(v,right)));ey=Math.max(ey,Math.abs(dot(v,up)))}const scale=Math.min((w-40)/(2*Math.max(.001,ex)),(h-40)/(2*Math.max(.001,ey)))*this.zoom;
  const projectPoint=p=>{const v=p.map((x,i)=>x-center[i]);return [w/2+this.pan[0]+dot(v,right)*scale,h/2+this.pan[1]-dot(v,up)*scale,dot(v,depth)]};
  const visible=this.buffers.filter(item=>item!==this.stockLines||showStock);
  if(this.gl){
   const gl=this.gl;gl.viewport(0,0,canvas.width,canvas.height);gl.clearColor(.055,.09,.12,1);gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);gl.enable(gl.DEPTH_TEST);gl.depthFunc(gl.LEQUAL);gl.useProgram(this.program);
   const sx=2*scale/w,sy2=2*scale/h,sz=-1/(radius*4);const matrix=new Float32Array([right[0]*sx,up[0]*sy2,depth[0]*sz,0,right[1]*sx,up[1]*sy2,depth[1]*sz,0,right[2]*sx,up[2]*sy2,depth[2]*sz,0,-dot(center,right)*sx+this.pan[0]*2/w,-dot(center,up)*sy2-this.pan[1]*2/h,-dot(center,depth)*sz,1]);gl.uniformMatrix4fv(this.locations.matrix,false,matrix);
   for(const item of visible){
    let drawBuffer=item.buffer,drawCount=item.count,mode=item.lines?gl.LINES:gl.TRIANGLES;
    if(wireframe&&!item.lines){if(!item.wireBuffer){const stride=Math.max(1,Math.ceil(item.count/3/12000)),edges=[];for(let i=0;i<item.count;i+=3*stride)for(const j of [0,1,1,2,2,0])edges.push(...item.data.subarray((i+j)*6,(i+j)*6+6));const wireData=new Float32Array(edges);item.wireBuffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,item.wireBuffer);gl.bufferData(gl.ARRAY_BUFFER,wireData,gl.STATIC_DRAW);item.wireCount=wireData.length/6}drawBuffer=item.wireBuffer;drawCount=item.wireCount;mode=gl.LINES}
    gl.bindBuffer(gl.ARRAY_BUFFER,drawBuffer);gl.enableVertexAttribArray(this.locations.position);gl.enableVertexAttribArray(this.locations.normal);gl.vertexAttribPointer(this.locations.position,3,gl.FLOAT,false,24,0);gl.vertexAttribPointer(this.locations.normal,3,gl.FLOAT,false,24,12);gl.uniform3fv(this.locations.color,item.color);gl.uniform1f(this.locations.shaded,item.lines||wireframe?0:1);let count=drawCount;const index=this.pathBuffers?.indexOf(item);if(index>=0)count=this.pathPrefixes[index][Math.floor(path.moves.length*progress/100)];gl.drawArrays(mode,0,count)}
   const move=path?.moves[Math.floor(path.moves.length*progress/100)-1];if(move&&(filter===null||move.operation===filter)){gl.bindBuffer(gl.ARRAY_BUFFER,this.markerBuffer);gl.bufferSubData(gl.ARRAY_BUFFER,0,new Float32Array([...move.to,0,0,0]));gl.vertexAttribPointer(this.locations.position,3,gl.FLOAT,false,24,0);gl.vertexAttribPointer(this.locations.normal,3,gl.FLOAT,false,24,12);gl.uniform3fv(this.locations.color,[1,1,1]);gl.uniform1f(this.locations.shaded,0);gl.disable(gl.DEPTH_TEST);gl.drawArrays(gl.POINTS,0,1)}
   canvas.dataset.renderer='webgl';
  }else if(this.ctx){
   const ctx=this.ctx;ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,w,h);const faces=[];
   for(const item of visible){if(item.lines)continue;const stride=Math.max(1,Math.ceil(item.count/3/12000));for(let i=0;i<item.count;i+=3*stride){const p=[0,1,2].map(j=>projectPoint(Array.from(item.data.subarray((i+j)*6,(i+j)*6+3))));faces.push({p,color:item.color,z:p.reduce((n,v)=>n+v[2],0)/3})}}
   faces.sort((a,b)=>a.z-b.z);for(const face of faces){ctx.beginPath();face.p.forEach((v,i)=>i?ctx.lineTo(v[0],v[1]):ctx.moveTo(v[0],v[1]));ctx.closePath();ctx.fillStyle=`rgb(${face.color.map(v=>Math.round(v*200)).join(',')})`;if(wireframe)ctx.stroke();else ctx.fill()}
   for(const item of visible.filter(x=>x.lines)){const line=new Path2D();let count=item.count;const index=this.pathBuffers?.indexOf(item);if(index>=0)count=this.pathPrefixes[index][Math.floor(path.moves.length*progress/100)];for(let i=0;i<count;i+=2){const a=projectPoint(Array.from(item.data.subarray(i*6,i*6+3))),b=projectPoint(Array.from(item.data.subarray((i+1)*6,(i+1)*6+3)));line.moveTo(a[0],a[1]);line.lineTo(b[0],b[1])}ctx.strokeStyle=`rgb(${item.color.map(v=>Math.round(v*255)).join(',')})`;ctx.stroke(line)}
   const move=path?.moves[Math.floor(path.moves.length*progress/100)-1];if(move&&(filter===null||move.operation===filter)){const p=projectPoint(move.to);ctx.fillStyle='#fff';ctx.beginPath();ctx.arc(p[0],p[1],4,0,Math.PI*2);ctx.fill()}
   canvas.dataset.renderer='canvas';
  }else this.onStatus('Graphics are unavailable in this browser. Enable graphics acceleration and reload.',true);
  canvas.dataset.triangles=String(this.triangleCount);canvas.dataset.zoom=String(this.zoom);canvas.dataset.yaw=String(this.yaw);
  return {triangles:this.triangleCount,fallback:!this.gl};
 }
}
