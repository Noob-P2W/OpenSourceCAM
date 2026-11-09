import {BUILD_ID} from './build.js';
const $=id=>document.getElementById(id);
function setPane(pane){document.body.dataset.pane=pane;document.querySelectorAll('[data-pane-target]').forEach(button=>button.setAttribute('aria-pressed',String(button.dataset.paneTarget===pane)));requestAnimationFrame(()=>draw())}
document.querySelectorAll('[data-pane-target]').forEach(button=>button.onclick=()=>{setPane(button.dataset.paneTarget);window.scrollTo({top:0,behavior:'instant'})});
$('preview-toggle').onclick=()=>{const visible=document.body.dataset.preview!=='true';document.body.dataset.preview=String(visible);$('preview-toggle').setAttribute('aria-pressed',String(visible));requestAnimationFrame(()=>draw())};
document.addEventListener('click',event=>{const menu=document.querySelector('.file-menu');if(menu.open&&!menu.contains(event.target))menu.open=false});
document.addEventListener('keydown',event=>{if(event.key==='Escape'){const menu=document.querySelector('.file-menu');if(menu.open){menu.open=false;menu.querySelector('summary').focus()}}});
document.querySelectorAll('[data-go-pane]').forEach(button=>button.onclick=()=>{setPane(button.dataset.goPane);window.scrollTo({top:0,behavior:'instant'})});
const worker=new Worker(new URL('./worker.js',import.meta.url),{type:'module'});
let sequence=0,pending=new Map(),project,selected=0,path=null,revision=0,busy=false;
let workspacePreview=[],machineProfiles=[],serializedProject=null;
function projectJSON(){return serializedProject??=JSON.stringify(project)}
function refreshEditor(){if(!project)return;const data=projectJSON();const open=$('advanced-editor').open;$('json').value=(open||data.length<512*1024)?data:'';$('json').placeholder='Open this editor to load project JSON. Large models use extra memory in text form.'}
let canUndo=false,canRedo=false,lastState,autosaved=false;
function historyButtons(){$('undo').disabled=busy||!canUndo;$('redo').disabled=busy||!canRedo}
function applyView(view){project=view.project;serializedProject=null;workspacePreview=view.workspace_preview||[];canUndo=view.can_undo;canRedo=view.can_redo}
function persist(){try{const data=projectJSON();const previous=localStorage.getItem('oc-project');let backupWarning=false;if(previous&&previous!==data){try{localStorage.setItem('oc-backup',previous)}catch{backupWarning=true}}localStorage.setItem('oc-project',data);autosaved=true;$('saved').textContent=backupWarning?'Saved on this device · previous backup could not be refreshed':'Saved on this device';return true}catch{autosaved=false;$('saved').textContent='Not saved — download a backup';status('Browser storage unavailable or full. Download a backup before closing.',true);return false}}
async function sessionEdit(edit){applyView(await call('session-edit',{edit}));if(edit.kind==='move_operation')selected=edit.target;if(edit.kind==='duplicate_operation')selected=edit.index+1;if(edit.kind==='feature_operation')selected=project.operations.length-1;await changed(false);render()}
worker.onmessage=({data})=>{const p=pending.get(data.id);if(!p)return;if(data.progress){status(data.progress);return}pending.delete(data.id);data.error?p.reject(Error(data.error)):p.resolve(data.value)};
worker.onerror=()=>{for(const p of pending.values())p.reject(Error('CAM engine failed to load. Rebuild or reload the app.'));pending.clear()};
function call(action,extra={}){return new Promise((resolve,reject)=>{const id=++sequence;pending.set(id,{resolve,reject});const message={id,action,...(['session-start','normalize','draft'].includes(action)?{project:projectJSON()}:{}),...extra};worker.postMessage(message,extra.file?[extra.file.bytes]:[])})}
function status(text,error=false){$('status').textContent=text;$('status').classList.toggle('error',error)}
async function task(fn){if(busy)return;busy=true;document.querySelectorAll('button,input,select,textarea').forEach(b=>b.disabled=true);try{await fn()}catch(e){status(e.message,true)}finally{busy=false;document.querySelectorAll('button,input,select,textarea').forEach(b=>b.disabled=false);historyButtons()}}
async function changed(record=true){
 if(record){serializedProject=null;autosaved=false;$('saved').textContent='Saving…';try{applyView(await call('session-edit',{edit:{kind:'replace',project}}))}catch(e){project=JSON.parse(lastState);serializedProject=lastState;render();persist();throw e}}
 lastState=projectJSON();revision++;path=null;status('Project changed · generate to refresh paths');$('stats').textContent='Changed · generate to refresh';$('result').textContent='';$('features').replaceChildren();refreshEditor();persist();historyButtons();draw()
}
function field(parent,label,obj,key,type='number',choices){const wrap=document.createElement('label');wrap.textContent=label;const input=document.createElement(choices?'select':'input');input.setAttribute('aria-label',label);if(choices){for(const [value,name] of choices){const o=document.createElement('option');o.value=value;o.textContent=name;input.append(o)}}else{input.type=type;if(type==='number')input.step='any'}if(type==='checkbox')input.checked=obj[key];else input.value=obj[key]??'';input.onchange=()=>task(async()=>{obj[key]=type==='id'?(input.value===''?null:Number(input.value)):type==='number'?input.valueAsNumber:type==='checkbox'?input.checked:input.value;await changed();render()});wrap.append(input);parent.append(wrap)}
function render(){if(!project)return;selected=Math.max(0,Math.min(selected,project.operations.length-1));$('title').textContent=project.name;$('title').title=project.name;$('count').textContent=project.operations.length;$('setup').replaceChildren();field($('setup'),'Project name',project,'name','text');for(const [k,n] of [['width','Stock X · mm'],['height','Stock Y · mm'],['thickness','Stock Z · mm']])field($('setup'),n,project.stock,k);field($('setup'),'Clearance · mm',project,'clearance');
if(project.part)for(let i=0;i<3;i++)field($('setup'),`Part offset ${'XYZ'[i]} · mm`,project.part.offset,i);
renderSetup();$('operations').replaceChildren();project.operations.forEach((op,i)=>{const b=document.createElement('button');b.className='op'+(i===selected?' selected':'');b.textContent=`${String(i+1).padStart(2,'0')}  ${op.name}${op.enabled?'':' · disabled'}`;b.onclick=()=>{selected=i;render()};$('operations').append(b)});$('editor').replaceChildren();const op=project.operations[selected];if(op){field($('editor'),'Name',op,'name','text');field($('editor'),'Enabled',op,'enabled','checkbox');field($('editor'),'Strategy',op,'strategy','text',[['pocket','Rectangular pocket'],['outside_profile','Outside profile'],['inside_profile','Inside profile'],['drill','Peck drilling'],['facing','Facing'],['circular_pocket','Circular pocket'],['stock_rough','Stock roughing'],['surface_rough','3D roughing'],['parallel_finish','Parallel finish']]);if(project.tool_library.tools.length){field($('editor'),'Operation tool',op,'tool_id','id',[['','Use setup active tool'],...project.tool_library.tools.filter(t=>t.uses.includes(project.process)).map(t=>[String(t.id),t.name])]);const tool=project.tool_library.tools.find(t=>t.id===(op.tool_id??project.active_tool));if(tool)field($('editor'),'Operation preset',op,'preset_id','id',[['','Tool default preset'],...tool.presets.map(p=>[String(p.id),p.name])])}for(const k of ['x','y','width','height','depth'])field($('editor'),`${k} · mm`,op,k);const del=document.createElement('button');del.textContent='Delete operation';del.onclick=()=>task(()=>sessionEdit({kind:'delete_operation',index:selected}));$('editor').append(del);for(const [label,kind,target] of [['Duplicate operation','duplicate_operation',null],['Move operation up','move_operation',selected-1],['Move operation down','move_operation',selected+1]]){const button=document.createElement('button');button.textContent=label;button.onclick=()=>task(()=>sessionEdit({kind,index:selected,...(target===null?{}:{target})}));$('editor').append(button)}}refreshEditor();draw()}
async function adopt(p){project=p;serializedProject=null;selected=0;await changed();render()}
function download(text,name){const url=URL.createObjectURL(new Blob([text],{type:'text/plain'}));const a=document.createElement('a');a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),10000)}
function filename(ext){return (project.name.replace(/[^a-z0-9_-]+/gi,'-')||'project')+ext}
$('demo').onclick=()=>task(async()=>{if(project&&!confirm('Replace current workspace? Download your project first if needed.'))return;await adopt(await call('sample'));status('Tray demo loaded')});
$('new').onclick=()=>task(async()=>{if(project&&!confirm('Replace current workspace?'))return;const p=await call('sample');p.name='Untitled milling project';p.operations=[];await adopt(p);status('New project')});
$('save').onclick=()=>task(async()=>{const data=await call('save');download(data,filename('.cam.json'));status('Project downloaded')});
$('open').onchange=()=>task(async()=>{const file=$('open').files[0];if(!file)return;if(file.size>128*1024*1024)throw Error('Project exceeds 128 MiB');const p=await call('normalize',{project:await file.text()});if(project&&!confirm('Replace current workspace with this project?'))return;await adopt(p);status('Project opened');$('open').value=''});
$('model').onchange=()=>task(async()=>{const file=$('model').files[0];if(!file)return;$('model').value='';if(file.size>32*1024*1024)throw Error('Model exceeds 32 MiB');const role=$('import-role').value;if(!confirm(role==='part'?'Create a new project fitted to this part?':'Add this reference model to the current setup?'))return;status('Importing model…');const imported=await call('import',{file:{name:file.name,bytes:await file.arrayBuffer()},role,inches:$('units').value==='inch'});applyView(imported);selected=0;await changed(false);render();status(`Model imported. Review stock fitting, CNC, vise and tooling in Setup.${autosaved?'':' Autosave is unavailable: download a project backup before closing.'}`,!autosaved)});
$('add').onclick=()=>task(async()=>{selected=project.operations.length;await sessionEdit({kind:'add_operation'})});
$('apply').onclick=()=>task(async()=>{await adopt(await call('normalize',{project:$('json').value}));status('Project settings applied')});
$('generate').onclick=()=>task(async()=>{status('Generating and checking paths…');const rev=revision;const p=await call('generate');if(rev!==revision){status('Project changed during generation. Generate again.');return}path=p;$('stats').textContent=`${p.moves.length} moves · ${p.cut.toFixed(0)} mm cutting · ~${Math.round(p.seconds)} s`;$('progress').value=100;draw();status('Paths generated and checked')});
$('export').onclick=()=>task(async()=>{const gcode=await call('export');download(gcode,filename('.nc'));status('G-code generated from current project and downloaded')});
$('simulate').onclick=()=>task(async()=>{status('Simulating stock removal…');const rev=revision;const s=await call('simulate');if(rev!==revision)return;const v=s.summary;$('result').textContent=`Sampled stock removal\nRemoved: ${v.removed_volume_mm3.toFixed(1)} mm³\nRemaining: ${v.remaining_volume_mm3.toFixed(1)} mm³\nGrid: ${v.resolution.join(' × ')}\n${v.notes.join('\n')}`;status('Stock simulation complete')});
for(const action of ['plan','surface'])$(action).onclick=()=>task(async()=>{status('Planning model-linked operations…');const rev=revision;const p=await call(action);if(rev!==revision)return;await adopt(p);status('Operations planned. Generate to inspect paths.')});
let drawPending=false;
const previewPaths=new WeakMap();
function draw(){if(drawPending)return;drawPending=true;requestAnimationFrame(()=>{drawPending=false;paint()})}
function paint(){
 if(!project)return;const c=$('canvas'),w=c.clientWidth,h=c.clientHeight;if(!w||!h)return;
 const dpr=Math.min(devicePixelRatio||1,2,Math.sqrt(4000000/(w*h)));
 const width=Math.max(1,Math.round(w*dpr)),height=Math.max(1,Math.round(h*dpr));
 if(c.width!==width)c.width=width;if(c.height!==height)c.height=height;
 const ctx=c.getContext('2d');if(!ctx)return;ctx.setTransform(dpr,0,0,dpr,0,0);ctx.clearRect(0,0,w,h);
const sx=project.stock.width,sy=project.stock.height;if(!Number.isFinite(sx)||!Number.isFinite(sy)||sx<=0||sy<=0)return;const iso=$('view').value==='iso',scale=Math.min((w-45)/(iso?sx+sy:sx),(h-55)/(iso?(sx+sy)*.45+project.stock.thickness:sy));function point([x,y,z=0]){return iso?[w/2+(x-y-(sx-sy)/2)*scale,h/2+((x+y-(sx+sy)/2)*.45-z)*scale]:[w/2+(x-sx/2)*scale,h/2-(y-sy/2)*scale]}
function line(a,b,color){ctx.strokeStyle=color;ctx.beginPath();ctx.moveTo(...point(a));ctx.lineTo(...point(b));ctx.stroke()}
const corners=[[0,0,0],[sx,0,0],[sx,sy,0],[0,sy,0]];ctx.fillStyle='#233d4a';ctx.beginPath();corners.forEach((p,i)=>i?ctx.lineTo(...point(p)):ctx.moveTo(...point(p)));ctx.closePath();ctx.fill();for(let i=0;i<4;i++)line(corners[i],corners[(i+1)%4],'#527180');

 const total=(project.part?.visible?project.part.geometry.mesh.triangles.length:0)+project.workspace.filter(m=>m.part.visible).reduce((n,m)=>n+m.part.geometry.mesh.triangles.length,0);
 let shown=0;
 function meshPath(mesh,transform,color,count,extra=''){
  if(shown>=12000)return;const limit=Math.min(12000-shown,Math.max(1,Math.floor(12000*count/Math.max(1,total))));
  const stride=Math.max(1,Math.ceil(mesh.triangles.length/limit));
  const key=[w,h,iso,sx,sy,project.stock.thickness,stride,extra].join(':');
  let entry=previewPaths.get(mesh);
  if(!entry||entry.key!==key){
   const path=new Path2D();let triangles=0;
   for(let t=0;t<mesh.triangles.length;t+=stride){const tri=mesh.triangles[t];const p=tri.map(i=>point(transform(mesh.vertices[i])));path.moveTo(...p[0]);path.lineTo(...p[1]);path.lineTo(...p[2]);path.closePath();triangles++}
   entry={key,path,triangles};previewPaths.set(mesh,entry);
  }
  ctx.strokeStyle=color;ctx.stroke(entry.path);shown+=entry.triangles;
 }
 ctx.globalAlpha=.5;
 if(project.part?.visible){const part=project.part,f=part.units==='inches'?25.4:1;meshPath(part.geometry.mesh,v=>[v[0]*f+part.offset[0],v[1]*f+part.offset[1],v[2]*f+part.offset[2]],'#c1d0dd',part.geometry.mesh.triangles.length,[f,...part.offset].join(','))}
 for(const model of workspacePreview){const source=project.workspace.filter(m=>m.part.visible).find(m=>m.id===model.id);meshPath(model,v=>v,model.kind==='machine'?'#647c92':'#d4ad70',source?.part.geometry.mesh.triangles.length||model.triangles.length)}
 ctx.globalAlpha=1;
 $('preview-detail').textContent=shown<total?`Lightweight preview: ${shown.toLocaleString()} of ${total.toLocaleString()} triangles. Full geometry is retained for CAM and project files.`:'';
 if(path){const n=Math.floor(path.moves.length*Number($('progress').value)/100);const cuts=new Path2D(),rapids=new Path2D();for(let i=0;i<n;i++){const m=path.moves[i],p=m.rapid?rapids:cuts;p.moveTo(...point(m.from));p.lineTo(...point(m.to))}ctx.strokeStyle='#ad8854';ctx.stroke(rapids);ctx.strokeStyle='#55dfc3';ctx.stroke(cuts);if(n){const p=point(path.moves[n-1].to);ctx.fillStyle='#fff';ctx.beginPath();ctx.arc(...p,4,0,Math.PI*2);ctx.fill()}}
 ctx.fillStyle='#91aebb';ctx.font='11px system-ui';ctx.fillText(`${sx} × ${sy} mm`,12,h-14);
}
$('view').onchange=draw;$('progress').oninput=draw;new ResizeObserver(draw).observe($('canvas'));
let installEvent;window.addEventListener('beforeinstallprompt',e=>{e.preventDefault();installEvent=e;$('install').hidden=false});$('install').onclick=async()=>{await installEvent?.prompt();$('install').hidden=true};
function connection(){$('connection').textContent=navigator.onLine?'Local workspace':'Offline workspace'}window.addEventListener('online',connection);window.addEventListener('offline',connection);connection();
function setupGroup(root,title,open){const details=document.createElement('details');details.open=!!open;const summary=document.createElement('summary');summary.textContent=title;const fields=document.createElement('div');fields.className='setup-fields';details.append(summary,fields);root.append(details);return fields}
function renderSetup(){
 const assets=$('asset-list');assets.replaceChildren();const names=[...(project.part?[`Part: ${project.part.geometry.source_name}`]:[]),...project.workspace.map(m=>`${m.kind}: ${m.name}`),...machineProfiles.map(p=>`CNC profile (this session): ${p.machine.name}`),...(project.tool_library.tools.length?[`Tool library: ${project.tool_library.tools.length} definitions`]:[])];if(!names.length)names.push('No models or libraries loaded yet.');for(const name of names){const p=document.createElement('p');p.textContent=name;assets.append(p)}

 const machineRoot=$('machine-setup'),machineOpen=machineRoot.querySelector('details')?.open;machineRoot.replaceChildren();let m=machineRoot;
 const select=document.createElement('select');select.setAttribute('aria-label','CNC profile');select.add(new Option(project.machine?.name||'Current CNC',''));machineProfiles.forEach((p,i)=>select.add(new Option(p.machine.name,String(i))));select.onchange=()=>{if(select.value!=='')task(()=>sessionEdit({kind:'apply_machine',profile:machineProfiles[Number(select.value)]}))};m.append(select);m=setupGroup(machineRoot,'Machine limits & controller settings',machineOpen);
 if(project.machine){field(m,'CNC name',project.machine,'name','text');for(const k of ['axis_min','axis_max','work_origin'])for(let i=0;i<3;i++)field(m,`${k.replaceAll('_',' ')} ${'XYZ'[i]} · mm`,project.machine[k],i);field(m,'Enforce travel limits',project.machine,'enforce_limits','checkbox');field(m,'Check tool reach',project.machine,'check_tool_reach','checkbox');for(const k of ['rapid_feed','max_feed','max_spindle_rpm'])field(m,({rapid_feed:'Rapid feed · mm/min',max_feed:'Maximum feed · mm/min',max_spindle_rpm:'Maximum spindle · RPM'})[k],project.machine,k)}
 field(m,'Postprocessor',project.post,'processor','text',[['generic','Generic metric'],['grbl','GRBL'],['mach3','Mach3'],['linuxcnc','LinuxCNC']]);field(m,'Work offset',project.post,'work_offset','text',['G54','G55','G56','G57','G58','G59'].map(v=>[v,v]));field(m,'Coolant',project.post,'coolant','text',[['off','Off'],['flood','Flood']]);
 const work=$('workholding');const openModels=new Set([...work.querySelectorAll('details[open]')].map(el=>el.dataset.modelId));work.replaceChildren();if(!project.workspace.length){const p=document.createElement('p');p.textContent='No reference models loaded. Import a vise, fixture or CNC reference geometry in Import.';work.append(p)}
 for(const model of project.workspace){const box=document.createElement('details');box.dataset.modelId=String(model.id);box.open=openModels.has(String(model.id));const summary=document.createElement('summary');summary.textContent=`${model.kind}: ${model.name}`;box.append(summary);field(box,'Visible',model.part,'visible','checkbox');field(box,'Coordinate frame',model,'frame','text',[['work','Work coordinates'],['machine','Machine coordinates']]);for(let i=0;i<3;i++){field(box,`${model.name} offset ${'XYZ'[i]} · mm`,model.part.offset,i);field(box,`${model.name} rotation ${'XYZ'[i]} · degrees`,model.rotation,i)}const remove=document.createElement('button');remove.textContent='Remove reference model';remove.onclick=()=>task(()=>sessionEdit({kind:'remove_workspace',id:model.id}));box.append(remove);work.append(box)}
 const toolsRoot=$('tool-setup'),toolOpen=toolsRoot.querySelector('details')?.open;toolsRoot.replaceChildren();let tools=toolsRoot;if(!project.tool_library.tools.length){field(toolsRoot,'Tool name',project.tool,'name','text');tools=setupGroup(toolsRoot,'Tool dimensions & cutting data',toolOpen);for(const [k,n] of [['diameter','Tool diameter · mm'],['feed','Cut feed · mm/min'],['plunge','Plunge · mm/min'],['spindle_rpm','Spindle · RPM'],['step_down','Step down · mm'],['step_over','Step over · fraction']])field(tools,n,project.tool,k);const b=document.createElement('button');b.textContent='Create library tool from current tool';b.onclick=()=>task(()=>sessionEdit({kind:'initialize_tools'}));toolsRoot.append(b)}else{
 field(tools,'Active tool',project,'active_tool','id',[['','Choose a tool'],...project.tool_library.tools.filter(t=>t.uses.includes(project.process)).map(t=>[String(t.id),t.name])]);const tool=project.tool_library.tools.find(t=>t.id===project.active_tool);if(tool){if(!tool.presets.some(p=>p.id===project.active_preset)){const hint=document.createElement('p');hint.textContent='Select a cutting preset before planning.';tools.append(hint)}field(tools,'Cutting preset',project,'active_preset','id',[['','Choose a preset'],...tool.presets.map(p=>[String(p.id),p.name])]);tools=setupGroup(toolsRoot,'Tool dimensions & cutting data',toolOpen);for(const key of ['diameter','flute_length','stickout'])if(key in tool.geometry)field(tools,`${key.replaceAll('_',' ')} · mm`,tool.geometry,key);const preset=tool.presets.find(p=>p.id===project.active_preset);if(preset)for(const key of ['cut_feed','plunge_feed','spindle_rpm','step_down','step_over'])if(key in preset.values)field(tools,({cut_feed:'Cut feed · mm/min',plunge_feed:'Plunge feed · mm/min',spindle_rpm:'Spindle · RPM',step_down:'Step down · mm',step_over:'Step over · %'})[key],preset.values,key)} }
}
$('fit-stock').onclick=()=>task(async()=>{await sessionEdit({kind:'fit_stock',margin:$('stock-margin').valueAsNumber});status('Stock fitted to part. Review existing operations and fixture positions before regenerating.')});
$('machine-file').onchange=()=>task(async()=>{const f=$('machine-file').files[0];if(!f)return;$('machine-file').value='';if(f.size>8*1024*1024)throw Error('CNC profile exceeds 8 MiB');const profile=JSON.parse(await f.text());if(typeof profile.machine?.name!=='string')throw Error('Choose a .cam-machine.json CNC profile');machineProfiles.push(profile);render();status('CNC profile imported. Select it under Setup → CNC machine.')});
$('tools-file').onchange=()=>task(async()=>{const f=$('tools-file').files[0];if(!f)return;$('tools-file').value='';if(f.size>8*1024*1024)throw Error('Tool library exceeds 8 MiB');await sessionEdit({kind:'import_tools',data:await f.text()});status('Tool library imported. Select an active tool and cutting preset in Setup.')});
$('recognize').onclick=()=>task(async()=>{
 const container=$('features');container.replaceChildren();
 const message=text=>{const p=document.createElement('p');p.textContent=text;container.append(p)};
 status('Recognizing supported features…');
 let report;
 try{report=await call('recognize')}catch(error){
  message('Recognition could not inspect this model.');message(error.message);
  message('Review the error above. For mesh errors, check that the STL is a closed solid with consistent face orientation; repair it in your CAD/mesh application and reimport it. STEP is also available if you have the original CAD model.');
  throw error;
 }
 const count=report.features.length;
 message(count?`${count} candidate features found. Review dimensions and accessibility before adding operations.`:'No supported features found. This recognizer looks for openings on the highest horizontal face, along the Z axis. Sideways features, curved surfaces, tapers, counterbores and pockets with islands are not currently supported.');
 if(report.skipped_openings)message(`${report.skipped_openings} top-face openings were skipped because their geometry is unsupported.`);
 if(report.machining_error)message(`Complete stock planning unavailable: ${report.machining_error}`);
 const details=document.createElement('details'),summary=document.createElement('summary');summary.textContent='Recognition limits';details.append(summary);
 for(const text of report.notes){const p=document.createElement('p');p.textContent=text;details.append(p)}container.append(details);
 report.features.forEach((feature,index)=>{const b=document.createElement('button');b.textContent=`Add ${feature.kind.replaceAll('_',' ')} · depth ${(feature.opening_z-feature.bottom_z).toFixed(2)} mm`;b.onclick=()=>task(async()=>{await sessionEdit({kind:'feature_operation',index});status('Feature operation added disabled. Review tooling, depth and placement before enabling it.')});container.append(b)});
 status(count?'Recognition complete. Candidates require review; choose suitable tooling in Setup before adding.':'Recognition complete: no supported features found. See the explanation in Operations.');
});
$('advanced-editor').addEventListener('toggle',refreshEditor);
$('version').textContent=`Build ${BUILD_ID}`;
$('undo').onclick=()=>task(()=>sessionEdit({kind:'undo'}));
$('redo').onclick=()=>task(()=>sessionEdit({kind:'redo'}));
$('backup').onclick=()=>download(projectJSON(),filename('.draft.json'));
$('recover').onclick=()=>task(async()=>{const data=localStorage.getItem('oc-backup');if(!data)throw Error('No previous autosave is available');const p=await call('draft',{project:data});if(confirm('Restore the previous autosave? The current workspace remains available through Undo.')){await adopt(p);status('Previous autosave restored')}});
window.addEventListener('beforeunload',e=>{if(!autosaved){e.preventDefault();e.returnValue=''}});
await task(async()=>{let saved,failed=false;try{saved=localStorage.getItem('oc-project')}catch{failed=true}if(saved){try{project=await call('draft',{project:saved}).catch(()=>call('normalize',{project:saved}))}catch{failed=true;$('recovery').hidden=false;$('raw-backup').onclick=()=>download(saved,'recovered-workspace.draft.json')}}if(!project)project=await call('sample');applyView(await call('session-start'));lastState=projectJSON();render();autosaved=!failed;status(failed?'Saved workspace could not be loaded. It has been preserved; download it using Recover draft.':'Ready · All machining calculations run on this device',failed);$('saved').textContent=failed?'Recovery required':'Workspace restored';});
if('serviceWorker'in navigator){
 let controlled=!!navigator.serviceWorker.controller,activating=false;
 navigator.serviceWorker.addEventListener('controllerchange',()=>{if(controlled&&!activating){location.reload();return}controlled=true;$('update').hidden=true});
 navigator.serviceWorker.register('./sw.js',{updateViaCache:'none'}).then(async reg=>{
  function offer(){if(reg.waiting&&reg.active){$('update').hidden=false;$('update').textContent='Update available · reload'}else if(!activating){$('update').hidden=true}}
  offer();reg.addEventListener('updatefound',()=>{reg.installing?.addEventListener('statechange',offer)});
  $('update').onclick=async()=>{
   if(busy){status('Finish the current operation before updating.',true);return}
   if(activating)return;
   if(!persist()){status('Download a backup before updating; autosave failed.',true);return}
   activating=true;$('update').disabled=true;$('update').textContent='Applying update…';status('Your workspace is saved. Applying the update…');
   try{
    // Resolve the current waiting worker, not a reference from an earlier install.
    const current=await navigator.serviceWorker.getRegistration();
    let next=current?.waiting;
    if(!next&&current){await current.update();next=current.waiting}
    if(!next){throw Error('No downloaded update is waiting. Close all OpenSourceCAM tabs and reopen the app to check again. Your workspace is saved.')}
    for(let round=0;round<4;round++){
     await new Promise((resolve,reject)=>{
      const timer=setTimeout(()=>{next.removeEventListener('statechange',check);reject(Error('The update did not activate. Close other OpenSourceCAM tabs and retry. Your workspace is saved.'))},12000);
      function check(){if(next.state==='activated'||next.state==='redundant'){clearTimeout(timer);next.removeEventListener('statechange',check);resolve()}}
      next.addEventListener('statechange',check);next.postMessage({type:'ACTIVATE'});check();
     });
     // Let registration state settle before deciding whether a newer download superseded it.
     await new Promise(resolve=>setTimeout(resolve,250));
     const latest=await navigator.serviceWorker.getRegistration();
     if(latest.installing){
      const installing=latest.installing;
      await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{installing.removeEventListener('statechange',check);reject(Error('A newer update is still downloading. Please retry shortly.'))},15000);function check(){if(installing.state==='installed'||installing.state==='redundant'){clearTimeout(timer);installing.removeEventListener('statechange',check);resolve()}}installing.addEventListener('statechange',check);check()});
     }
     const settled=await navigator.serviceWorker.getRegistration();
     if(settled.waiting){next=settled.waiting;continue}
     if(settled.active?.state==='activated'){location.reload();return}
    }
    throw Error('A newer update is still arriving. Please retry shortly. Your workspace is saved.');
   }catch(error){activating=false;$('update').disabled=false;$('update').textContent='Retry update';status(error.message,true)}
  };
  await navigator.serviceWorker.ready;$('offline').textContent='· Offline cache installed';
 }).catch(error=>{$('offline').textContent='· Offline cache unavailable';status(`Offline/update setup failed: ${error.message}`,true)});
}
