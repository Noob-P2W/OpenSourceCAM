import {initScene} from './scene.js';
import {initVise} from './vise.js';
import {operationColor,attachHelp,initEyes,installStaticHelp,motionOutputField} from './help.js';
import {loadWorkspace,readSnapshot,saveWorkspace} from './storage.js';
import {BUILD_ID} from './build.js';
import {Viewport} from './viewport.js';
import {initShell} from './shell.js';
import {confirmAction,showContextMenu} from './dialogs.js';
let shell,viseUI,presentation={};
const $=id=>document.getElementById(id);
initEyes();installStaticHelp();
function setPane(pane){document.body.dataset.pane=pane;shell?.navigate(pane);requestAnimationFrame(()=>draw())}
$('preview-toggle').onclick=()=>{const visible=document.body.dataset.preview!=='true';document.body.dataset.preview=String(visible);$('preview-toggle').textContent=visible?'Hide preview':'Show preview';$('preview-toggle').setAttribute('aria-pressed',String(visible));requestAnimationFrame(()=>draw())};
document.addEventListener('click',event=>{const menu=document.querySelector('.file-menu');if(menu.open&&!menu.contains(event.target))menu.open=false});
document.addEventListener('keydown',event=>{if(event.key==='Escape'){const menu=document.querySelector('.file-menu');if(menu.open){menu.open=false;menu.querySelector('summary').focus()}}});
document.querySelectorAll('[data-go-pane]').forEach(button=>button.onclick=()=>{setPane(button.dataset.goPane);window.scrollTo({top:0,behavior:'instant'})});
let worker=createEngine(),jobCheckpoint=null,restarting=null;
let sequence=0,pending=new Map(),project,selected=0,path=null,revision=0,busy=false;
let sceneUI;let inspectionMode=false,inspection=null;
let workspacePreview=[],serializedProject=null,catalog={processes:[],tools:[]},simulation=null,highlight=null,hoverSelection=null,toolFocus=null,playing=false,playbackFrame=null,playbackProgress=100,lastPlaybackTime=0;
let stockRequest=false,stockWanted=null,stockEpoch=0,stockTimer=null;
let playbackToolPath=null,playbackToolRequest=null,playbackToolWanted=null;
const playbackToolCache=new Map();
const geometryState={key:null,faces:null,face:null,report:null,feature:null,selectedFeature:null,survey:null,exterior:false};
function geometryKey(){return project?.part?`${presentation.part_fingerprint}:${project.setup_face??'top'}`:null}
function updateGeometryState(){const key=geometryKey();if(geometryState.key!==key){inspection=null;if(!project.part){inspectionMode=false;document.body.dataset.featureInspect='false'}Object.assign(geometryState,{key,faces:null,face:null,report:null,feature:null,selectedFeature:null,survey:null,exterior:false});$('features').replaceChildren();$('feature-tags').replaceChildren();$('feature-count').textContent='Not scanned';highlight=null}}

const viewport=new Viewport($('canvas'),()=>draw(),(text,error)=>status(text,error));
function projectJSON(){return serializedProject??=JSON.stringify(project)}
function refreshEditor(){if(!project)return;$('json').dataset.dirty='false';const data=projectJSON();const open=$('advanced-editor').open;$('json').value=(open||data.length<512*1024)?data:'';$('json').placeholder='Open this editor to load project JSON. Large models use extra memory in text form.'}
let canUndo=false,canRedo=false,lastState,autosaved=false,fieldsDirty=false,fieldTimer;
function scheduleFields(){clearTimeout(fieldTimer);fieldTimer=setTimeout(()=>{if(document.activeElement?.matches('input:not([type=checkbox]),textarea')){scheduleFields();return}task(async()=>{})},350)}
function markFields(){fieldsDirty=true;$('show-simulation').checked=false;serializedProject=null;autosaved=false;path=null;simulation=null;clearStockRequests();pausePlayback();$('stats').textContent='Changed · generate to refresh';$('saved').textContent='Saving…';$('saved').dataset.state='saving';historyButtons();draw();scheduleFields()}
function historyButtons(){if($('pick-finish'))$('pick-finish').disabled=busy||$('pick-finish').dataset.ready!=='true';$('fit-vise').disabled=busy||!project?.workspace.some(m=>m.kind==='vise');$('undo').disabled=busy||(!canUndo&&!fieldsDirty);$('redo').disabled=busy||fieldsDirty||!canRedo;const planned=project&&project.process!=='milling';for(const id of ['surface','plan','generate','export','simulate','nc-preview','add'])$(id).disabled=busy||planned;$('replace-library').disabled=busy||project?.operations.length>0;if(project?.operations.length)$('replace-library').checked=false;$('save-stock').disabled=busy||!simulation;$('play').disabled=busy||!path?.moves.length;$('simulation-requirement').hidden=!!path?.moves.length&&!path.stock_preview_error;$('simulation-requirement').textContent=path?.stock_preview_error||'Generate valid toolpaths to enable stock simulation.';for(const id of ['simulate','show-simulation'])$(id).disabled=busy||!path?.moves.length||!!path.stock_preview_error}
function displayView(view){presentation={part_geometry_key:view.part_geometry_key,setup_allowances:view.setup_allowances,setup_margin:view.setup_margin,part_fingerprint:view.part_fingerprint,part_transform:view.part_transform,stock_outline:view.stock_outline,stock_definition:view.stock_definition,setup_issue:view.setup_issue,clamping_stock:view.clamping_stock};Object.defineProperty(project,'_display',{value:presentation,configurable:true});workspacePreview=view.workspace_preview||[];canUndo=view.can_undo;canRedo=view.can_redo}
function applyView(view){project=view.project;serializedProject=null;displayView(view)}
let saveSequence=0;
async function persist(){
 const sequence=++saveSequence,data=projectJSON();autosaved=false;$('saved').textContent='Saving…';$('saved').dataset.state='saving';
 try{await saveWorkspace(data);if(sequence!==saveSequence)return false;autosaved=!fieldsDirty&&data===projectJSON();$('saved').dataset.state=autosaved?'saved':'saving';$('saved').textContent=autosaved?'Autosaved on this device':'Saving…';return autosaved}
 catch(error){if(sequence===saveSequence){autosaved=false;$('saved').dataset.state='error';$('saved').textContent='Not saved — download a backup';status(`Autosave unavailable: ${error.message}. Download a project backup before closing.`,true)}return false}
}
async function sessionEdit(edit){const fixedVise=edit.kind==='clamp_stock_in_vise'||edit.kind==='clamp_stock_in_vise_setup'||(edit.kind==='open_vise'&&project.workspace.find(m=>m.id===edit.id)?.frame==='machine');const fixedFace=edit.fixed??project.workspace.find(m=>m.id===edit.id)?.clamp?.fixed;const reference=workspacePreview.find(m=>m.id===edit.id);const fixedVertex=reference?.triangles[fixedFace]?.[0];const anchor=fixedVise&&fixedVertex!==undefined&&viewport.projectWorld?viewport.projectWorld(reference.vertices[fixedVertex]):null;const oldJaw=project.workspace.find(m=>m.id===edit.id)?.clamp?.closed_by||0;applyView(await call('session-edit',{edit}));if(anchor){const next=workspacePreview.find(m=>m.id===edit.id);viewport.keepPointOnScreen(next.vertices[fixedVertex],anchor)}if(['configure_vise','configure_vise_group','clamp_vise','clamp_vise_setup','clamp_stock_in_vise','clamp_stock_in_vise_setup','open_vise','add_vise_profile'].includes(edit.kind))highlight=null;if(['clamp_vise','clamp_vise_setup','clamp_stock_in_vise','clamp_stock_in_vise_setup','open_vise'].includes(edit.kind)){const preview=workspacePreview.find(m=>m.id===edit.id);if(preview){const distance=['clamp_vise','clamp_vise_setup','clamp_stock_in_vise','clamp_stock_in_vise_setup'].includes(edit.kind)?(project.workspace.find(m=>m.id===edit.id)?.clamp?.closed_by||0):-oldJaw;preview.jaw_motion={from:preview.jaw_axis.map(v=>v*distance),start:null}}}if(edit.kind==='move_operation')selected=edit.target;if(edit.kind==='duplicate_operation')selected=edit.index+1;if(edit.kind==='feature_operation')selected=project.operations.length-1;await changed(false);render()}
function createEngine(){const engine=new Worker(new URL('./worker.js',import.meta.url),{type:'module'});
engine.onmessage=({data})=>{const p=pending.get(data.id);if(!p)return;if(data.progress){status(data.progress);return}pending.delete(data.id);data.error?p.reject(Error(data.error)):p.resolve(data.value)};
engine.onerror=()=>{for(const p of pending.values())p.reject(Error('CAM engine failed to load. Rebuild or reload the app.'));pending.clear()};
return engine;}
function engineCall(action,extra={}){return new Promise((resolve,reject)=>{const id=++sequence;pending.set(id,{resolve,reject});const message={id,action,...(['session-start','normalize','draft'].includes(action)?{project:projectJSON()}:{}),...extra};worker.postMessage(message,extra.file?[extra.file.bytes]:[])})}
async function call(action,extra={}){
 if(busy&&lastState&&['generate','recognize','simulate-current','plan-model','import','stock-obj','geometry'].includes(action)){
  const history=await engineCall('history-checkpoint');jobCheckpoint={history,project:lastState};$('job-controls').hidden=false;$('cancel-job').disabled=false;
 }
 return engineCall(action,extra);
}
$('cancel-job').onclick=()=>{
 if(!busy||!jobCheckpoint||restarting)return;
 $('cancel-job').disabled=true;const saved=jobCheckpoint;worker.onmessage=null;worker.onerror=null;worker.terminate();for(const p of pending.values()){const error=Error('Calculation cancelled. Workspace and undo history preserved.');error.name='AbortError';p.reject(error);}pending.clear();worker=createEngine();
 restarting=engineCall('restore-session',{project:saved.project,history:saved.history}).then(view=>{canUndo=view.can_undo;canRedo=view.can_redo}).catch(error=>{status(`Engine recovery failed: ${error.message}. Download a backup and reload.`,true)});
};
function status(text,error=false){$('status').textContent=text;$('status').classList.toggle('error',error);document.dispatchEvent(new CustomEvent('cam-status',{detail:{text,error}}))}
async function task(fn){if(busy)return;pausePlayback();clearStockRequests();busy=true;const disabled=new Map([...document.querySelectorAll('button,input,select,textarea')].map(b=>[b,b.disabled]));for(const b of disabled.keys())b.disabled=true;try{if(fieldsDirty){clearTimeout(fieldTimer);fieldsDirty=false;await changed(true,true);render()}await fn()}catch(e){status(e.message,e.name!=='AbortError')}finally{if(restarting)await restarting;restarting=null;jobCheckpoint=null;$('job-controls').hidden=true;busy=false;for(const [b,was] of disabled)if(b.isConnected)b.disabled=was;historyButtons();shell?.render()}}
let autoGenerateTimer=null;
function scheduleGeneration(){clearTimeout(autoGenerateTimer);if(!shell?.prefs().autoGenerate)return;const rev=revision;autoGenerateTimer=setTimeout(function run(){if(rev!==revision||!shell.prefs().autoGenerate)return;if(busy||fieldsDirty){autoGenerateTimer=setTimeout(run,250);return;}if(project?.process==='milling'&&project.operations.some(o=>o.enabled)&&!path)$('generate').click();},600);}
async function changed(record=true,keepPresentation=false){
 if(record){serializedProject=null;autosaved=false;$('saved').textContent='Saving…';try{const view=await call('session-edit',{edit:{kind:'replace',project}});if(keepPresentation){displayView(view);viewport.scene=null}else applyView(view)}catch(e){project=JSON.parse(lastState);serializedProject=lastState;render();await persist();throw e}}
 operationSelection.clear();selectionAnchor=selected;viewport.controls.hoveredOperation=null;
 lastState=projectJSON();revision++;viewport.controls.hiddenOperations?.clear();path=null;simulation=null;clearStockRequests();pausePlayback();$('show-simulation').checked=false;$('nc-code').textContent='';$('planning-notes').textContent='';status('Project changed · generate to refresh paths');$('stats').textContent='Changed · generate to refresh';$('result').textContent='';updateGeometryState();refreshEditor();await persist();historyButtons();draw();scheduleGeneration()
}
function field(parent,label,obj,key,type='number',choices,factor=1){const wrap=document.createElement('label');wrap.textContent=label;const input=document.createElement(choices?'select':'input');input.setAttribute('aria-label',label);if(choices){for(const [value,name] of choices){const o=document.createElement('option');o.value=value;o.textContent=name;input.append(o)}}else{input.type=['optional','integer','optional-integer'].includes(type)?'number':type;if(input.type==='number')input.step=type.includes('integer')?'1':'any';if(type.startsWith('optional'))input.placeholder='Use default'}if(type==='checkbox')input.checked=obj[key];else input.value=typeof obj[key]==='number'?obj[key]/factor:obj[key]??'';const update=()=>{if(busy)return;obj[key]=type==='id'?(input.value===''?null:Number(input.value)):['optional','optional-integer'].includes(type)?(input.value===''?null:input.valueAsNumber*factor):['number','integer'].includes(type)?input.valueAsNumber*factor:type==='checkbox'?input.checked:input.value;if(obj===project&&key==='active_tool')project.active_preset=project.tool_library.tools.find(t=>t.id===project.active_tool)?.default_preset??null;if(key==='tool_id')obj.preset_id=null;markFields()};input.oninput=update;input.onchange=update;wrap.append(input);parent.append(wrap);attachHelp(input,key,label);return input}
let operationSelection=new Set(),selectionAnchor=0;
function selectedOperations(){return operationSelection.size?operationSelection:new Set(project?.operations[selected]?[selected]:[]);}
function selectOperation(index,event={}){
 selectionAnchor=Math.max(0,Math.min(selectionAnchor,project.operations.length-1));for(const i of operationSelection)if(i>=project.operations.length)operationSelection.delete(i);
 if(event.shiftKey){if(!(event.ctrlKey||event.metaKey))operationSelection.clear();for(let i=Math.min(selectionAnchor,index);i<=Math.max(selectionAnchor,index);i++)operationSelection.add(i);}
 else if(event.ctrlKey||event.metaKey){if(operationSelection.size===0)operationSelection.add(selected);if(operationSelection.has(index))operationSelection.delete(index);else operationSelection.add(index);if(!operationSelection.size)operationSelection.add(index);selectionAnchor=index;}
 else {operationSelection=new Set([index]);selectionAnchor=index;}
 selected=operationSelection.has(index)?index:[...operationSelection].at(-1);draw();
}
let timelinePath,timelineOperation;
function updateTimeline(){
 if(timelinePath===path)return;timelinePath=path;timelineOperation=undefined;const stops=[];
 if(!path?.seconds){$('progress').style.background='';return;}
 let blocks=[];for(const m of path.moves){let b=blocks.at(-1);if(!b||b.operation!==m.operation){b={operation:m.operation,start:m.start_seconds,end:m.end_seconds};blocks.push(b);}else b.end=m.end_seconds;}
 for(const b of blocks){if(b.end<=b.start)continue;const color=operationColor(project.operations[b.operation]?.strategy),start=b.start/path.seconds*100,end=b.end/path.seconds*100;stops.push(`${color} ${start}%`);if(b!==blocks.at(-1))stops.push(`${color} calc(${end}% - 1px)`,`#fff calc(${end}% - 1px)`,`#fff ${end}%`);else stops.push(`${color} ${end}%`);}
 $('progress').style.background=`linear-gradient(to right,${stops.join(',')})`;
}
function render(){if(!project)return;selected=Math.max(0,Math.min(selected,project.operations.length-1));$('title').textContent=project.name;$('title').title=project.name;renderSetup();renderExtras();refreshEditor();shell?.render();sceneUI?.render();draw()}
async function adopt(p){project=p;serializedProject=null;selected=0;await changed();render()}
function download(text,name){const url=URL.createObjectURL(new Blob([text],{type:'text/plain'}));const a=document.createElement('a');a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),10000)}
function filename(ext){return (project.name.replace(/[^a-z0-9_-]+/gi,'-')||'project')+ext}
$('new').onclick=showHome;
$('save').onclick=()=>task(async()=>{const data=await call('save');download(data,filename('.cam.json'));status('Project downloaded')});
$('open').onchange=()=>task(async()=>{const file=$('open').files[0];if(!file)return;$('open').value='';if(file.size>128*1024*1024)throw Error('Project exceeds 128 MiB');const p=await call('normalize',{project:await file.text()});if(project&&!await confirmAction({title:'Open project?',message:'Replace the current workspace with the selected project?',accept:'Open project',destructive:true}))return;await adopt(p);showWorkspace();viewport.fit();status('Project opened');$('open').value=''});
$('model').onchange=()=>task(async()=>{const file=$('model').files[0];if(!file)return;$('model').value='';if(file.size>32*1024*1024)throw Error('Model exceeds 32 MiB');const role=$('import-role').value;if(!await confirmAction({title:role==='part'?'Import workpiece?':'Add reference model?',message:role==='part'?'Replace the workpiece and refit stock? Reference models, CNC and tool library are retained. Existing machining operations will be cleared.':'Add this model to the current setup for positioning and workholding?',accept:role==='part'?'Import workpiece':'Add model',destructive:role==='part'}))return;shell.close('import-dialog');status('Importing model…');const imported=await call('import',{file:{name:file.name,bytes:await file.arrayBuffer()},role,inches:$('units').value==='inch'});applyView(imported);selected=0;await changed(false);render();shell.afterImport(role);document.body.dataset.preview='true';$('preview-toggle').textContent='Hide preview';$('preview-toggle').setAttribute('aria-pressed','true');const geometry=(role==='part'?project.part:project.workspace.at(-1)?.part)?.geometry,repaired=geometry?.removed_degenerate_triangles||0;status(`Model imported.${geometry?.bodies?.length?` ${geometry.bodies.length} CAD components retained.`:''}${repaired?` Auto-fixed ${repaired} degenerate triangles.`:''} Review stock fitting, CNC, vise and tooling in Setup.${autosaved?'':' Autosave is unavailable: download a project backup before closing.'}`,!autosaved)});
$('add').onclick=()=>task(async()=>{selected=project.operations.length;await sessionEdit({kind:'add_strategy',strategy:$('new-strategy').value})});
$('apply').onclick=()=>task(async()=>{await adopt(await call('normalize',{project:$('json').value}));status('Project settings applied')});
$('generate').onclick=()=>task(async()=>{status('Generating and checking paths…');const rev=revision;const p=await call('generate');if(rev!==revision){status('Project changed during generation. Generate again.');return}path=p;resetPlaybackTools();if(p.moves.length)queuePlaybackTool(p.moves[0].operation);simulation=null;clearStockRequests();$('stats').textContent=`${p.moves.length} moves · ${p.cut.toFixed(0)} mm cutting · ~${Math.round(p.seconds)} s`;$('progress').value=100;if(p.stock_preview_error)$('show-simulation').checked=false;if($('show-simulation').checked)requestStockPreview(true);draw();status(`Paths generated and checked${p.stock_preview_error?' · '+p.stock_preview_error:''}`)});
$('export').onclick=()=>task(async()=>{const gcode=await call('export');download(gcode,filename('.nc'));status('G-code generated from current project and downloaded')});
$('simulate').onclick=()=>task(async()=>{if(!path?.moves.length)throw Error('Generate valid toolpaths before simulating stock.');pausePlayback();clearStockRequests();shell.close('stock-simulation-dialog');status('Simulating stock removal…');const rev=revision;const s=await call('simulate-current',{progress:Number($('progress').value)/100,resolution:Number($('simulation-detail').value),budget:simulationBudget(),conservative:$('conservative-stock').checked});if(rev!==revision)return;simulation={...s,resolution:Number($('simulation-detail').value),budget:simulationBudget(),conservative:$('conservative-stock').checked};$('show-simulation').checked=true;draw();const v=s.summary;$('result').textContent=`${v.conservative?'Conservative':'Sampled'} stock removal\nRemoved: ${v.removed_volume_mm3.toFixed(1)} mm³\nRemaining: ${v.remaining_volume_mm3.toFixed(1)} mm³\nGrid: ${v.resolution.join(' × ')}\n${v.notes.join('\n')}`;status('Stock simulation complete')});
for(const action of ['plan','surface'])$(action).onclick=()=>task(async()=>{
 shell.close('planning-dialog');status('Planning model-linked operations…');
 const options={fit_stock:!$('use-setup-stock').checked,xy_margin:$('plan-margin').valueAsNumber,top_allowance:$('plan-top').valueAsNumber,backing:$('plan-backing').valueAsNumber,replace_operations:$('replace-operations').checked,face:$('plan-face').checked,outside:$('plan-outside').checked,cavities:$('plan-cavities').checked};
 if(Object.values(options).some(v=>typeof v==='number'&&!Number.isFinite(v)))throw Error('Enter valid blank allowances before planning.');
 const planned=await call('plan-model',{planner:action,options});applyView(planned.view);selected=0;await changed(false);render();viewport.fit();$('planning-notes').textContent=planned.notes.join('\n');status('Operations planned. Generate to inspect paths.');
});
let drawPending=false;
function draw(){if(drawPending)return;drawPending=true;requestAnimationFrame(()=>{drawPending=false;paint()})}
function paint(){updateTimeline();
 if(!project)return;
const visibleProgress=$('show-simulation').checked&&simulation&&path?simulation.summary.progress*100:Number($('progress').value);
$('simulation-off').hidden=!($('show-simulation').checked||playing||simulation);$('simulation-off').disabled=busy;$('simulation-off-menu').disabled=busy||!($('show-simulation').checked||playing||simulation);
$('canvas').dataset.stockProgress=$('show-simulation').checked&&simulation?String(simulation.summary.progress*100):'';if(path)$('playback-clock').textContent=`${formatTime(visibleProgress/100*path.seconds)} / ${formatTime(path.seconds)}`;
const result=viewport.draw(project,workspacePreview,path,visibleProgress,$('show-simulation').checked?simulation:null,hoverSelection||highlight,$('wireframe').checked,$('show-stock').checked,$('selected-path').checked?selectedOperations():null,$('show-tool').checked?playbackTool:null);
 if(result&&timelineOperation!==result.timelineOperation){timelineOperation=result.timelineOperation;$('progress').dataset.operation=timelineOperation??'';$('progress').title=timelineOperation==null?'Path playback':`${timelineOperation+1}. ${project.operations[timelineOperation]?.name||'Operation'}`;}
 positionFeatureTags(result);
 if(result){const entry=result.operation===null?null:playbackToolCache.get(playbackToolKey(result.operation));$('playback-tool-state').textContent=result.toolChange?`Tool exchange · estimated duration · ${result.changePositionKnown?'configured change station':'controller/operator route; no station configured'}`:!$('show-tool').checked||result.operation===null?'':result.tool?`${result.tool.number===null?'Legacy tool':`T${result.tool.number}`} · ${result.tool.name} · cutting Ø${result.tool.cutting_diameter} mm${result.tool.shaft_diameter!==result.tool.cutting_diameter?` · shaft Ø${result.tool.shaft_diameter} mm`:""}`:entry?.error?`Tool preview unavailable: ${entry.error}`:'Loading tool model…'}
 if(result){if($('show-simulation').checked&&simulation)$('stock-playback-state').textContent=`Stock ${(simulation.summary.progress*100).toFixed(1)}% · ${simulation.resolution} cells · spacing ${Math.max(...simulation.summary.cell_size).toFixed(3)} mm${stockRequest?' · updating':simulation.preview?' · playback preview':' · inspected snapshot'}`;$('preview-detail').textContent=result.fallback?`Software preview: complete model surfaces. Full geometry is retained for CAM and project files.`:`${result.triangles.toLocaleString()} model triangles · Full geometry is retained for CAM and project files.`;}
}
$('fit-view').onclick=()=>viewport.fit();$('zoom-in').onclick=()=>viewport.magnify(1.25);$('zoom-out').onclick=()=>viewport.magnify(.8);
$('show-paths').oninput=()=>{viewport.controls.paths=$('show-paths').checked;draw()};
for(const id of ['wireframe','show-stock','selected-path','show-tool'])$(id).oninput=draw;
try{const saved=localStorage.getItem('oc-stock-resolution');if([...$('simulation-detail').options].some(o=>o.value===saved))$('simulation-detail').value=saved}catch{}
$('conservative-stock').onchange=()=>{clearStockRequests();if(path)path.preview_initial=null;simulation=null;requestStockPreview(true);draw()};
$('simulation-detail').onchange=()=>{try{localStorage.setItem('oc-stock-resolution',$('simulation-detail').value)}catch{};clearStockRequests();if(path)path.preview_initial=null;requestStockPreview();draw()};
$('show-simulation').oninput=()=>{clearStockRequests();if($('show-simulation').checked)requestStockPreview(true);else{simulation=null;$('save-stock').disabled=true;$('stock-playback-state').textContent=''}draw()};
$('simulation-off-menu').onclick=$('simulation-off').onclick=()=>{
 pausePlayback();$('show-simulation').checked=false;clearStockRequests();simulation=null;
 if(path)path.preview_initial=null;
 $('result').textContent='';$('save-stock').disabled=true;viewport.scene=null;draw();
 status('Simulation off. Original model restored; generated paths retained.');
};
$('progress').oninput=()=>{pausePlayback();requestStockPreview(true);draw()};
new ResizeObserver(draw).observe($('canvas'));
// Load library assemblies on demand. Keep four CPU meshes and only the active
// tool's GPU buffers, independent of stock snapshots and animation frames.
function playbackToolKey(index){return project.operations[index]?.tool_id??project.active_tool??'legacy'}
function resetPlaybackTools(){playbackToolPath=path;playbackToolCache.clear();playbackToolWanted=null}
function queuePlaybackTool(index,prefetch=false){
 if(!path||!project.operations[index])return;
 if(playbackToolPath!==path)resetPlaybackTools();
 const key=playbackToolKey(index);
 if(playbackToolCache.has(key)||(playbackToolRequest?.path===path&&playbackToolRequest.key===key))return;
 if(prefetch&&(playbackToolRequest||playbackToolWanted))return;
 playbackToolWanted={path,revision,index,key};pumpPlaybackTool();
}
function pumpPlaybackTool(){
 if(playbackToolRequest||!playbackToolWanted)return;
 const request=playbackToolWanted;playbackToolWanted=null;playbackToolRequest=request;
 const current=()=>request.path===path&&request.path===playbackToolPath&&request.revision===revision;
 const retain=value=>{playbackToolCache.set(request.key,value);while(playbackToolCache.size>4)playbackToolCache.delete(playbackToolCache.keys().next().value);draw()};
 engineCall('playback-tool',{index:request.index}).then(tool=>{
  if(!current())return;
  retain(tool);
 }).catch(error=>{if(current())retain({error:error.message})}).finally(()=>{playbackToolRequest=null;pumpPlaybackTool()});
}
function playbackTool(index){
 if(playbackToolPath!==path)resetPlaybackTools();
 const key=playbackToolKey(index),tool=playbackToolCache.get(key);
 if(!tool){queuePlaybackTool(index);return null}
 playbackToolCache.delete(key);playbackToolCache.set(key,tool);
 // Preload the next distinct tool without queuing an entire job's assemblies.
 const next=project.operations.findIndex((op,i)=>i>index&&op.enabled&&playbackToolKey(i)!==key);
 if(next>=0)queuePlaybackTool(next,true);
 return tool.error?null:tool;
}
$('plan-margin').oninput=()=>{$('stock-margin').value=$('plan-margin').value};$('stock-margin').oninput=()=>{$('plan-margin').value=$('stock-margin').value};
// One background stock request at a time. Reset
// immediately to the shared uncut mesh on rewind; stale worker results cannot
// replace stock after a seek, project edit or a new calculation.
function clearStockRequests(){stockEpoch++;stockWanted=null;if(!path||!$('show-simulation').checked)$('stock-playback-state').textContent='';clearTimeout(stockTimer);stockTimer=null}
function requestStockPreview(reset=false){
 if(!$('show-simulation').checked||!path?.moves.length||path.stock_preview_error)return;
 const progress=Number($('progress').value)/100,resolution=Number($('simulation-detail').value),conservative=$('conservative-stock').checked;
 if(reset){clearStockRequests();simulation=(path.preview_initial?.resolution===resolution&&path.preview_initial?.conservative===conservative)?path.preview_initial:path.initial_stock?{...path.initial_stock,resolution:32,conservative,preview:true}:null;draw()}
 if(!reset&&simulation?.resolution===resolution&&simulation.conservative===conservative&&simulation.summary.progress===progress)return;
 if(progress===0&&(resolution===32||(path.preview_initial?.resolution===resolution&&path.preview_initial?.conservative===conservative))){stockWanted=null;$('stock-playback-state').textContent='Uncut stock · 0%';return}
 stockWanted={progress,resolution,conservative,epoch:stockEpoch,revision,path};
 $('stock-playback-state').textContent=`Stock ${((simulation?.summary.progress||0)*100).toFixed(1)}% · updating`;
 pumpStockPreview();
}
function simulationBudget(){const preference=shell.prefs().simulationBudget||'auto';if(preference!=='auto')return Number(preference);return (navigator.deviceMemory||0)>=4&&(navigator.hardwareConcurrency||0)>=4?100000000:50000000}
function pumpStockPreview(){
 if(stockRequest||!stockWanted)return;
 if(busy){if(stockTimer===null)stockTimer=setTimeout(()=>{stockTimer=null;pumpStockPreview()},100);return}
 const request=stockWanted;stockWanted=null;stockRequest=true;
 engineCall('stock-preview',{progress:request.progress,resolution:request.resolution,epoch:request.epoch,budget:simulationBudget(),conservative:request.conservative}).then(value=>{
  if(request.epoch!==stockEpoch||request.revision!==revision||request.path!==path||!$('show-simulation').checked)return;
  if(request.progress>Number($('progress').value)/100+1e-9)return;
  simulation={...value,resolution:request.resolution,conservative:request.conservative,preview:true};viewport.stageStockFrame(simulation);if(request.progress===0&&request.resolution>32)path.preview_initial=simulation;
  $('stock-playback-state').textContent=`Stock ${(value.summary.progress*100).toFixed(1)}% · playback preview`;
  $('save-stock').disabled=false;draw();
 }).catch(error=>{if(request.epoch===stockEpoch){clearStockRequests();pausePlayback();status(`Stock playback paused: ${error.message}`,true)}}).finally(()=>{stockRequest=false;pumpStockPreview()});
}
// Keep fractional playback state outside the slider: range inputs round to their step.
function pausePlayback(){
 playing=false;
 if(playbackFrame!==null)cancelAnimationFrame(playbackFrame);
 playbackFrame=null;
 $('play').textContent='Play paths';$('play').setAttribute('aria-pressed','false');
}
$('playback-speed').onchange=()=>{lastPlaybackTime=performance.now()};
function formatTime(seconds){const n=Math.floor(seconds);return `${Math.floor(n/3600)}:${String(Math.floor(n/60)%60).padStart(2,'0')}:${String(n%60).padStart(2,'0')}`}
function playbackTick(now){
 playbackFrame=null;
 if(!playing)return;
 playbackProgress=Math.min(100,playbackProgress+Math.max(0,now-lastPlaybackTime)/1000/Math.max(path?.seconds||0,0.001)*100*Number($('playback-speed').value));
 lastPlaybackTime=now;$('progress').value=playbackProgress;requestStockPreview();draw();
 if(playbackProgress===100)pausePlayback();
 else playbackFrame=requestAnimationFrame(playbackTick);
}
$('play').onclick=()=>{
 if(playing){pausePlayback();return}
 if(!path?.moves.length){status('Generate paths before playback.',true);return}
 playbackProgress=Number($('progress').value);
 const restart=playbackProgress>=100;
 if(restart)playbackProgress=0;
 $('progress').value=playbackProgress;requestStockPreview(restart);lastPlaybackTime=performance.now();playing=true;
 $('play').textContent='Pause paths';$('play').setAttribute('aria-pressed','true');
 draw();playbackFrame=requestAnimationFrame(playbackTick);
};
document.addEventListener('visibilitychange',()=>{if(document.hidden)pausePlayback()});
$('nc-preview').onclick=()=>task(async()=>{shell.close('inspect-dialog');const text=await call('export');shell.openInspect();$('nc-code').textContent=text.length>100000?text.slice(0,100000)+'\n… Preview truncated. Download G-code for the complete program.':text;status('G-code preview checked against the current project')});
$('save-stock').onclick=()=>task(async()=>{if(!simulation)throw Error('Run stock simulation first.');const data=await call('stock-obj',{progress:simulation.summary.progress,resolution:simulation.resolution,budget:simulationBudget(),conservative:!!simulation.conservative});download(data,filename('-stock.obj'));status('Remaining-stock mesh downloaded')});
$('save-machine').onclick=()=>task(async()=>{download(await call('machine-profile'),filename('.cam-machine.json'));status('CNC profile downloaded')});
let installEvent;window.addEventListener('beforeinstallprompt',e=>{e.preventDefault();installEvent=e;$('install').hidden=false});$('install').onclick=async()=>{await installEvent?.prompt();$('install').hidden=true};
function connection(){$('connection').textContent=navigator.onLine?'Local workspace':'Offline workspace'}window.addEventListener('online',connection);window.addEventListener('offline',connection);connection();
function setupGroup(root,title,open){const details=document.createElement('details');details.open=!!open;const summary=document.createElement('summary');summary.textContent=title;const fields=document.createElement('div');fields.className='setup-fields';details.append(summary,fields);root.append(details);return fields}
function renderSetup(){
 const assets=$('asset-list');assets.replaceChildren();const names=[...(project.part?[`Part: ${project.part.geometry.source_name}`]:[]),...project.workspace.map(m=>`${m.kind}: ${m.name}`),...(project.tool_library.tools.length?[`Tool library: ${project.tool_library.tools.length} definitions`]:[])];if(!names.length)names.push('No models or libraries loaded yet.');for(const name of names){const p=document.createElement('p');p.textContent=name;assets.append(p)}

 const machineRoot=$('machine-setup'),machineOpen=machineRoot.querySelector('details')?.open;machineRoot.replaceChildren();let m=machineRoot;
 m=setupGroup(machineRoot,'Machine limits & controller settings',machineOpen??true);
 if(project.machine){field(m,'CNC name',project.machine,'name','text');for(const k of ['axis_min','axis_max','work_origin'])for(let i=0;i<3;i++)field(m,`${k.replaceAll('_',' ')} ${'XYZ'[i]} · mm`,project.machine[k],i);field(m,'Enforce travel limits',project.machine,'enforce_limits','checkbox');field(m,'Check tool reach',project.machine,'check_tool_reach','checkbox');for(const k of ['rapid_feed','max_feed','max_spindle_rpm'])field(m,({rapid_feed:'Rapid feed · mm/min',max_feed:'Maximum feed · mm/min',max_spindle_rpm:'Maximum spindle · RPM'})[k],project.machine,k)}
 const processor=field(m,'Postprocessor',project.post,'processor','text',catalog.posts.filter(p=>p.id!=='klipper'||project.process==='printing_3d').map(p=>[p.id,p.label]));field(m,'Work offset',project.post,'work_offset','text',['G54','G55','G56','G57','G58','G59'].map(v=>[v,v]));field(m,'Coolant',project.post,'coolant','text',[['off','Off'],['flood','Flood']]);motionOutputField(field,m,project.post,processor,catalog.posts);field(m,'Use tool coolant settings',project.post,'use_tool_coolant','checkbox');
 const work=$('workholding');const openModels=new Set([...work.querySelectorAll('details[open]')].map(el=>el.dataset.modelId));work.replaceChildren();if(!project.workspace.length){const p=document.createElement('p');p.textContent='No reference models loaded. Import a vise, fixture or CNC reference geometry in Import.';work.append(p)}
 for(const model of project.workspace){const box=document.createElement('details');box.dataset.modelId=String(model.id);box.open=openModels.has(String(model.id));const summary=document.createElement('summary');summary.textContent=`${model.kind}: ${model.name}`;box.append(summary);field(box,'Visible',model.part,'visible','checkbox');field(box,'Coordinate frame',model,'frame','text',[['work','Work coordinates'],['machine','Machine coordinates']]);for(let i=0;i<3;i++){field(box,`${model.name} offset ${'XYZ'[i]} · mm`,model.part.offset,i);field(box,`${model.name} rotation ${'XYZ'[i]} · degrees`,model.rotation,i)}const remove=document.createElement('button');remove.textContent='Remove reference model';remove.onclick=()=>task(()=>sessionEdit({kind:'remove_workspace',id:model.id}));box.append(remove);work.append(box);if(model.kind==='vise'){const configure=document.createElement('button');configure.textContent=`${model.clamp?'Edit':'Set up'} clamping: ${model.name}`;configure.onclick=()=>viseUI.open(model.id);work.append(configure)}if(model.vise){const edit=document.createElement('button');edit.innerHTML='<span aria-hidden="true">↕</span> Reseat stock / edit jaw grip';edit.onclick=()=>shell.openVise(model);work.append(edit)}}
 const toolsRoot=$('tool-setup'),toolOpen=toolsRoot.querySelector('details')?.open;toolsRoot.replaceChildren();let tools=toolsRoot;if(!project.tool_library.tools.length){field(toolsRoot,'Tool name',project.tool,'name','text');tools=setupGroup(toolsRoot,'Tool dimensions & cutting data',toolOpen);for(const [k,n] of [['diameter','Tool diameter · mm'],['feed','Cut feed · mm/min'],['plunge','Plunge · mm/min'],['spindle_rpm','Spindle · RPM'],['step_down','Step down · mm'],['step_over','Step over · fraction']])field(tools,n,project.tool,k);const b=document.createElement('button');b.textContent='Create library tool from current tool';b.onclick=()=>task(()=>sessionEdit({kind:'initialize_tools'}));toolsRoot.append(b)}else{
 field(tools,'Active tool',project,'active_tool','id',[['','Choose a tool'],...project.tool_library.tools.filter(t=>t.uses.includes(project.process)).map(t=>[String(t.id),t.name])]);const tool=project.tool_library.tools.find(t=>t.id===project.active_tool);if(tool){if(!tool.presets.some(p=>p.id===project.active_preset)){const hint=document.createElement('p');hint.textContent='Select a cutting preset before planning.';tools.append(hint)}field(tools,'Cutting preset',project,'active_preset','id',[['','Choose a preset'],...tool.presets.map(p=>[String(p.id),p.name])]);tools=setupGroup(toolsRoot,'Tool dimensions & cutting data',toolOpen);for(const key of ['diameter','flute_length','stickout'])if(key in tool.geometry)field(tools,`${key.replaceAll('_',' ')} · mm`,tool.geometry,key);const preset=tool.presets.find(p=>p.id===project.active_preset);if(preset)for(const key of ['cut_feed','plunge_feed','spindle_rpm','step_down','step_over'])if(key in preset.values)field(tools,({cut_feed:'Cut feed · mm/min',plunge_feed:'Plunge feed · mm/min',spindle_rpm:'Spindle · RPM',step_down:'Step down · mm',step_over:'Step over · %'})[key],preset.values,key)} }
}
$('tools-file').onchange=()=>task(async()=>{const f=$('tools-file').files[0];if(!f)return;$('tools-file').value='';if(f.size>8*1024*1024)throw Error('Tool library exceeds 8 MiB');await sessionEdit({kind:'import_library',data:await f.text(),replace:$('replace-library').checked});status('Tool library imported. Select an active tool and cutting preset in Setup.')});

// Display the selected original triangles; do not invent walls or floors.
function featureHighlight(feature,outline=feature.outline){
 const mesh=project.part?.geometry.mesh,matrix=presentation.part_transform;
 const transform=v=>matrix?[0,1,2].map(i=>matrix[0][i]*v[0]+matrix[1][i]*v[1]+matrix[2][i]*v[2]+matrix[3][i]):v;
 const ids=feature.detail?.triangles||[],fill=ids=>ids.filter(i=>mesh?.triangles[i]).map(i=>mesh.triangles[i].map(v=>transform(mesh.vertices[v])));
 const selection={outline,fill:fill(ids)};
 return selection;
}
let pendingFeaturePick=null;
let inspectSerial=0;
function clearGeometrySelection(){inspectSerial++;hoverEpoch++;hoverSelection=null;$('canvas').dataset.hoverSelection='false';geometryState.face=null;geometryState.feature=null;geometryState.exterior=false;geometryState.selectedFeature=null;geometryState.selectedEntry=null;highlight=null;$('features').querySelectorAll('[aria-pressed=true]').forEach(b=>b.setAttribute('aria-pressed','false'));$('feature-tags').querySelectorAll('[aria-pressed=true]').forEach(b=>b.setAttribute('aria-pressed','false'));draw()}
let pickCancel=null,hoverEpoch=0;
function cancelPicking(){document.body.dataset.geometryPicking='false';document.body.dataset.edgePicking='false';if(viewport.controls.clampingStock){viewport.controls.clampingStock=false;viewport.scene=null;}pickCancel=null;$('pick-face').textContent='Cancel selection';pendingFeaturePick=null;viewport.onPick=null;viewport.onHover=null;hoverEpoch++;hoverSelection=null;$('canvas').dataset.hoverSelection='false';if($('pick-finish'))$('pick-finish').hidden=true;$('pick-face').hidden=true;$('selection-hint').hidden=true;draw()}
function beginPick(callback,label='Pick a face on the model',onCancel=null,preview=null){inspectSerial++;document.body.dataset.geometryPicking='true';pickCancel=onCancel;shell.hideBrowser();sceneUI?.close();
 shell.close('operation-dialog');$('show-simulation').checked=false;clearStockRequests();
 $('pick-face').hidden=false;$('selection-hint').hidden=false;$('selection-hint').textContent=label;$('selection-hint').dataset.unavailable='false';
 viewport.onPick=ray=>task(async()=>{hoverSelection=null;$('canvas').dataset.hoverSelection='false';if(await callback(ray)!==false)cancelPicking()});
 let wanted=null,inFlight=false;const epoch=++hoverEpoch;let serial=0;
 async function pumpHover(){if(inFlight||!wanted||epoch!==hoverEpoch)return;const request=wanted;wanted=null;inFlight=true;try{const selected=await preview(request.ray);if(epoch===hoverEpoch&&request.serial===serial){hoverSelection={...selected,hover:true};$('selection-hint').textContent=selected.unavailable||label;$('selection-hint').dataset.unavailable=String(!!selected.unavailable);$('canvas').dataset.hoverSelection='true';draw();}}catch{if(epoch===hoverEpoch&&request.serial===serial){hoverSelection=null;$('canvas').dataset.hoverSelection='false';$('selection-hint').textContent=label;$('selection-hint').dataset.unavailable='false';draw();}}finally{inFlight=false;if(wanted)pumpHover();}}
 viewport.onHover=preview?ray=>{serial++;if(!ray){wanted=null;hoverSelection=null;$('canvas').dataset.hoverSelection='false';$('selection-hint').textContent=label;$('selection-hint').dataset.unavailable='false';draw();return;}wanted={ray,serial};pumpHover();}:null;
 draw();status(label);
}
$('pick-face').onclick=()=>{const resume=pickCancel;cancelPicking();if(resume)resume();else shell.resumeOperation?.()};
async function geometryDraft(strategy,index){
 const feature=index!==undefined?geometryState.report?.features[index]:geometryState.selectedFeature;
 if(feature&&((feature.kind==='rectangular_pocket'&&['pocket','circular_pocket','inside_profile','slot','chamfer'].includes(strategy))||(feature.kind==='circular_hole'&&['pocket','circular_pocket','bore','circular','thread','drill','drill_once'].includes(strategy))))return call('geometry',{data:{action:'feature_geometry',feature,strategy}});
 if(strategy==='facing'&&geometryState.face)return call('geometry',{data:{action:'facing_draft',triangle:geometryState.face.id}});
 return null;
}
async function pickOperation(strategy,commit,edgeMode='boundary'){
 pendingFeaturePick={strategy,commit};let chainDraft=null;
 const edgeSelection=draft=>({outline:[],segments:[...draft.settings.path.slice(1).flatMap((p,i)=>[draft.settings.path[i],p]),...(draft.settings.closed?[draft.settings.path.at(-1),draft.settings.path[0]]:[])],fill:[]});
 const edgeStrategies=['trace','engrave','chamfer','inside_profile','outside_profile','stock_rough'];
 const query=async(ray,extend=false)=>{
  if(edgeStrategies.includes(strategy)){
   if(strategy==='chamfer'&&edgeMode==='boundary'){try{const feature=await engineCall('geometry',{data:{action:'pick_feature',...ray}});const draft=await engineCall('geometry',{data:{action:'feature_geometry',feature,strategy}});return {draft,selection:edgeSelection(draft)}}catch{}}
   const draft=await engineCall('geometry',{data:{action:'chain_draft',...ray,strategy,single_edge:edgeMode!=='boundary',existing_path:extend&&chainDraft?chainDraft.settings.path:[],edge_tolerance:12/(viewport.viewScale||1)}});return {draft,selection:edgeSelection(draft)};
  }
  if(['surface_smooth','area_rough'].includes(strategy)){const draft=await engineCall('geometry',{data:{action:'smooth_face_draft',roughing:strategy==='area_rough',...ray}});const face=await engineCall('geometry',{data:{action:'pick',...ray}});return {draft,selection:face};}
  if(strategy==='facing'){const face=await engineCall('geometry',{data:{action:'pick',...ray}});try{return {draft:await engineCall('geometry',{data:{action:'facing_draft',triangle:face.id}}),selection:face};}catch(error){return {selection:{...face,unavailable:error.message},unavailable:error.message};}}
  if(['pocket','circular_pocket'].includes(strategy)){
   try{const feature=await engineCall('geometry',{data:{action:'pick_feature',...ray}});return {draft:await engineCall('geometry',{data:{action:'feature_geometry',feature,strategy}}),selection:featureHighlight(feature)}}catch{
    const draft=await engineCall('geometry',{data:{action:'pocket_face_draft',...ray}});const face=await engineCall('geometry',{data:{action:'pick',...ray}});return {draft,selection:face};
   }
  }
  const feature=await engineCall('geometry',{data:{action:'pick_feature',...ray}});return {draft:await engineCall('geometry',{data:{action:'feature_geometry',feature,strategy}}),selection:featureHighlight(feature)};
 };
 const connected=edgeStrategies.includes(strategy)&&edgeMode==='connected';document.body.dataset.edgePicking=String(edgeStrategies.includes(strategy));
 beginPick(async ray=>{const result=await query(ray,connected);highlight=result.selection;draw();if(result.unavailable){status(result.unavailable,true);return false;}chainDraft=result.draft;if(connected){$('pick-finish').dataset.ready='true';$('pick-finish').disabled=false;return false;}await commit(result.draft);},connected?'Select connected edges, then Finish selection':edgeStrategies.includes(strategy)?'Select a highlighted edge or boundary chain':['surface_smooth','area_rough'].includes(strategy)?'Select a highlighted horizontal target face':'Select the highlighted machining feature',null,async ray=>(await query(ray,connected)).selection);
 if(connected){let finish=$('pick-finish');if(!finish){finish=document.createElement('button');finish.id='pick-finish';finish.textContent='Finish selection';$('pick-face').after(finish);}finish.hidden=false;finish.dataset.ready='false';finish.disabled=true;finish.onclick=()=>task(async()=>{if(chainDraft){await commit(chainDraft);cancelPicking();}});}
}

function featureText(entry){const f=entry.feature;return `${entry.label||f.kind.replaceAll('_',' ')} · ${f.diameter?`Ø ${f.diameter.toFixed(2)} mm`:`${(f.max[0]-f.min[0]).toFixed(2)} × ${(f.max[1]-f.min[1]).toFixed(2)} mm`} · depth ${(f.opening_z-f.bottom_z).toFixed(2)} mm`}
function selectRecognized(entry,index){inspectSerial++;if(inspectionMode){inspection=featureInspection(entry.feature);shell.render()}
 geometryState.selectedFeature=entry.current_orientation?entry.feature:null;geometryState.selectedEntry=index;highlight=featureHighlight(entry.feature,entry.outline);
 $('features').querySelectorAll('[data-feature-view]').forEach(b=>b.setAttribute('aria-pressed',String(Number(b.dataset.featureView)===index)));
 $('feature-tags').querySelectorAll('button').forEach(b=>b.setAttribute('aria-pressed',String(Number(b.dataset.featureTag)===index)));
 draw();status(`${featureText(entry)}${entry.machinable?'':' · inspection candidate; use a suitable 3D / dedicated strategy'}`);
 const pending=pendingFeaturePick;
 if(pending&&entry.current_orientation&&(entry.machinable||['slot','chamfer'].includes(pending.strategy))){task(async()=>{const op=await call('geometry',{data:{action:'feature_geometry',feature:entry.feature,strategy:pending.strategy}});await pending.commit(op);cancelPicking()})}
 else if(innerWidth<=700)shell.hideBrowser();
}
function renderFeatureTree(survey){
 const container=$('features');container.replaceChildren();const tags=$('feature-tags');tags.replaceChildren();
 $('feature-count').textContent=String(survey.features.length);$('feature-notes').textContent=survey.features.length?'Select an item here or a marker on the part.': 'No supported candidates found. Use 3D surface operations for unclassified surfaces.';
 for(const [index,entry] of survey.features.entries()){
  const card=document.createElement('div');card.className='feature-card';const button=document.createElement('button');button.className='tree-feature';button.dataset.featureView=String(index);button.textContent=featureText(entry);button.onclick=()=>selectRecognized(entry,index);card.append(button);if(!entry.current_orientation){const hint=document.createElement('small');hint.textContent='Other setup direction';card.append(hint)}
  if(entry.feature.detail?.profiles?.length>1){const meta=document.createElement('small');meta.textContent=`${entry.feature.detail.profiles.length} wall profile levels${entry.feature.detail.floor_z!==null&&entry.feature.detail.floor_z!==undefined?` · floor Z ${entry.feature.detail.floor_z.toFixed(2)} mm`:''}`;card.append(meta);}
  const other=entry.orientations.filter(o=>!o.current);if(other.length){const directions=document.createElement('details');const summary=document.createElement('summary');summary.textContent='Other setup directions';directions.append(summary);for(const orientation of other){const align=document.createElement('button');align.textContent='Orient this side upward…';align.onclick=()=>task(async()=>{if(!await confirmAction({title:'Change machining setup?',message:'Rotate the part and refit the stock for this direction? Existing linked paths need rebuilding. Reseat fixtures after changing the setup.',accept:'Orient & fit stock'}))return;await sessionEdit({kind:'set_machining_face',triangle:orientation.face,align:true});viewport.fit();await recognizeAll()});directions.append(align)}card.append(directions)}
  container.append(card);
  if(index<80&&entry.marker){const tag=document.createElement('button');tag.className='feature-tag';tag.dataset.featureTag=String(index);tag.title=featureText(entry);tag.setAttribute('aria-label',`Select ${featureText(entry)}`);tag.textContent=entry.feature.diameter?`⊙ ${entry.feature.diameter.toFixed(1)}`:'◇';tag.onclick=()=>selectRecognized(entry,index);tags.append(tag)}
 }

}
function positionFeatureTags(result){if(!result?.projectPoint||!geometryState.survey)return;const w=$('canvas').clientWidth,h=$('canvas').clientHeight;for(const tag of $('feature-tags').children){const entry=geometryState.survey.features[Number(tag.dataset.featureTag)],directions=entry.orientations.filter(o=>o.marker),opening=(directions.length?directions:[entry]).reduce((best,o)=>o.normal.reduce((n,v,i)=>n+v*result.camera[i],0)>best.normal.reduce((n,v,i)=>n+v*result.camera[i],0)?o:best),p=result.projectPoint(opening.marker),facing=opening.normal.reduce((n,v,i)=>n+v*result.camera[i],0);tag.hidden=facing<.1||p[0]<14||p[0]>w-14||p[1]<20||p[1]>h-14;tag.style.left=`${p[0]}px`;tag.style.top=`${p[1]}px`;}}
async function recognizeAll(){
 status('Inspecting features across model orientations and levels…');$('features').replaceChildren();$('feature-tags').replaceChildren();
 let survey;try{survey=await call('geometry',{data:{action:'survey'}})}catch(error){$('feature-notes').textContent=`Recognition could not inspect this model. ${error.message}`;shell.showFeatures();throw error;}
 geometryState.survey=survey;geometryState.selectedFeature=null;renderFeatureTree(survey);shell.showFeatures();draw();
 status(survey.features.length?'Recognition complete. Part features are listed in the project tree.':'Recognition complete: no supported features found. Use 3D surface operations.');
}

function sendToVise(){const vises=project.workspace.filter(m=>m.kind==='vise'),vise=vises.find(m=>m.id===viewport.fitTarget?.model)||vises[0];if(!vise)return;if(vise.vise){task(async()=>{await sessionEdit({kind:'place_example_vise',id:vise.id,spec:vise.vise});viewport.fit('all');status('Part and stock seated in the example vise.')});return}if(!vise.clamp){viseUI.open(vise.id);status('Select the jaw faces, then clamp the stock.');return}task(async()=>{await sessionEdit({kind:'clamp_stock_in_vise',id:vise.id,grip:vise.clamp.grip});viseUI.open(vise.id,false);viewport.controls.clampingStock=true;viewport.scene=null;draw();status('Part and stock seated in the vise. Stock is clamped.')})}
function toggleInspection(on){inspectionMode=on;inspection=null;document.body.dataset.featureInspect=String(on);if(on){cancelPicking();viseUI.suspend();sceneUI.close();if(innerWidth<700)shell.hideBrowser()}else clearGeometrySelection();shell.render();draw()}
function featureInspection(f){return{title:(f.detail?.shape&&f.detail.shape!=='prismatic'?f.detail.shape:f.kind).replaceAll('_',' '),fields:[...(f.diameter?[['Diameter',`${f.diameter.toFixed(3)} mm`]]:[]),['Depth',`${(f.opening_z-f.bottom_z).toFixed(3)} mm`],['Opening Z',`${f.opening_z.toFixed(3)} mm`],['Bottom Z',`${f.bottom_z.toFixed(3)} mm`],['Center',f.center.map(n=>n.toFixed(3)).join(', ')+' mm']]}}
viewport.onInspect=async (ray,pixel)=>{if(busy||!project.part)return;const rev=revision,serial=++inspectSerial;hoverSelection=null;try{
 let feature;try{feature=await engineCall('geometry',{data:{action:'pick_feature',...ray}})}catch{}
 if(rev!==revision||serial!==inspectSerial)return;
 if(feature){clearGeometrySelection();if(inspectionMode){inspection=featureInspection(feature);shell.render()}highlight=featureHighlight(feature);geometryState.selectedFeature=feature;status(`${feature.kind.replaceAll('_',' ')} · ${feature.diameter?`diameter ${feature.diameter.toFixed(3)} mm · `:''}depth ${(feature.opening_z-feature.bottom_z).toFixed(3)} mm · opening Z ${feature.opening_z.toFixed(3)} mm`)}
 else {const face=await engineCall('geometry',{data:{action:'pick',...ray}});if(rev!==revision||serial!==inspectSerial)return;clearGeometrySelection();geometryState.face=face;highlight=face;if(inspectionMode){inspection={title:face.planar_direction?'Planar surface':'Surface',fields:[['Area',`${face.area.toFixed(3)} mm²`],['Inclination',`${face.inclination_degrees.toFixed(2)}°`],['Normal',face.normal.map(n=>n.toFixed(4)).join(', ')],['Center',face.center.map(n=>n.toFixed(3)).join(', ')+' mm'],['Boundary edges',face.edge_lengths.length]]};if(pixel){let best=7,edge=-1;for(let i=0;i<face.segments.length;i+=2){const a=viewport.projectWorld(face.segments[i]),b=viewport.projectWorld(face.segments[i+1]),dx=b[0]-a[0],dy=b[1]-a[1],t=Math.max(0,Math.min(1,((pixel.x-a[0])*dx+(pixel.y-a[1])*dy)/(dx*dx+dy*dy||1))),distance=Math.hypot(pixel.x-a[0]-t*dx,pixel.y-a[1]-t*dy);if(distance<best){best=distance;edge=i/2}}if(edge>=0){inspection={title:'Surface boundary edge',fields:[['Length',`${face.edge_lengths[edge].toFixed(3)} mm`],['Start',face.segments[edge*2].map(n=>n.toFixed(3)).join(', ')+' mm'],['End',face.segments[edge*2+1].map(n=>n.toFixed(3)).join(', ')+' mm']]};highlight={segments:face.segments.slice(edge*2,edge*2+2),outline:[]}}}shell.render()}status(`Surface · area ${face.area.toFixed(2)} mm² · normal ${face.normal.map(n=>n.toFixed(2)).join(', ')} · Z ${face.z.toFixed(3)} mm`)}draw();
}catch(error){if(rev!==revision||serial!==inspectSerial)return;if(error.message.includes('No model face')){inspection=null;if(inspectionMode)shell.render();clearGeometrySelection();status('Selection cleared')}else status(error.message,true)}};
function operationActions(index){
 if(!selectedOperations().has(index)){selectOperation(index);render();}
 const op=project.operations[index];
 if(selectedOperations().has(index)&&selectedOperations().size>1){const indices=new Set(selectedOperations());return [
  {label:`Enable ${indices.size} selected operations`,action:()=>task(async()=>{for(const i of indices)project.operations[i].enabled=true;await changed();operationSelection=new Set(indices);render()})},
  {label:`Disable ${indices.size} selected operations`,action:()=>task(async()=>{for(const i of indices)project.operations[i].enabled=false;await changed();operationSelection=new Set(indices);render()})},
  {label:'Show selected paths',action:()=>{for(const i of indices)viewport.controls.hiddenOperations?.delete(i);render()}},
  {label:'Hide selected paths',action:()=>{viewport.controls.hiddenOperations??=new Set();for(const i of indices)viewport.controls.hiddenOperations.add(i);render()}},
 ];}
 return [
  {label:'Edit operation',action:()=>shell.openOperation(index)},
  {label:'Operation enabled',checked:op.enabled,action:()=>task(async()=>{op.enabled=!op.enabled;await changed();render()})},
  {label:'Duplicate operation',action:()=>task(()=>sessionEdit({kind:'duplicate_operation',index}))},
  {label:'Move operation up',disabled:index===0,action:()=>task(()=>sessionEdit({kind:'move_operation',index,target:index-1}))},
  {label:'Move operation down',disabled:index===project.operations.length-1,action:()=>task(()=>sessionEdit({kind:'move_operation',index,target:index+1}))},
  {label:'Delete operation',action:()=>task(()=>sessionEdit({kind:'delete_operation',index}))}
 ];
}
function showOperationMenu(index,x,y){if(busy)return;selected=index;render();showContextMenu(operationActions(index),x,y,document.querySelectorAll('.op')[index])}
const resizer=$('pane-resizer'),bench=document.querySelector('.workbench');
function resizeEditor(width){const max=Math.max(320,bench.clientWidth*.65),value=Math.round(Math.max(320,Math.min(max,width)));bench.style.setProperty('--editor-width',`${value}px`);resizer.setAttribute('aria-valuenow',String(value));resizer.setAttribute('aria-valuemax',String(Math.round(max)));try{sessionStorage.setItem('oc-editor-width',String(value))}catch{}draw()}
resizer.onpointerdown=event=>{resizer.setPointerCapture(event.pointerId);event.preventDefault()};resizer.onpointermove=event=>{if(resizer.hasPointerCapture(event.pointerId))resizeEditor(bench.getBoundingClientRect().right-event.clientX)};resizer.onpointerup=event=>{if(resizer.hasPointerCapture(event.pointerId))resizer.releasePointerCapture(event.pointerId)};
resizer.onkeydown=event=>{if(event.key==='ArrowLeft'||event.key==='ArrowRight'){event.preventDefault();resizeEditor(document.querySelector('.side-panels').clientWidth+(event.key==='ArrowLeft'?20:-20))}};
try{const width=Number(sessionStorage.getItem('oc-editor-width'));if(width>0)bench.style.setProperty('--editor-width',`${width}px`)}catch{}
const browserPanel=$('browser-panel'),browserHead=browserPanel.querySelector('.browser-heading');
const browserSizer=document.createElement('div');browserSizer.id='browser-resizer';browserSizer.className='browser-resizer';browserSizer.tabIndex=0;browserSizer.setAttribute('role','separator');browserSizer.setAttribute('aria-label','Resize project sidebar');browserSizer.setAttribute('aria-orientation','vertical');bench.append(browserSizer);
function resizeBrowser(width){const value=Math.round(Math.max(200,Math.min(bench.clientWidth*.5,width)));bench.style.setProperty('--browser-width',`${value}px`);browserPanel.style.width=document.body.dataset.browserFloating==='true'?`${value}px`:'';browserSizer.setAttribute('aria-valuenow',String(value));draw()}
browserSizer.onpointerdown=e=>{browserSizer.setPointerCapture(e.pointerId);e.preventDefault()};browserSizer.onpointermove=e=>{if(browserSizer.hasPointerCapture(e.pointerId))resizeBrowser(e.clientX-bench.getBoundingClientRect().left)};browserSizer.onpointerup=e=>browserSizer.releasePointerCapture(e.pointerId);browserSizer.onkeydown=e=>{if(['ArrowLeft','ArrowRight'].includes(e.key)){e.preventDefault();resizeBrowser(browserPanel.clientWidth+(e.key==='ArrowRight'?20:-20))}};
browserHead.title='Drag to move the project sidebar';
const dock=document.createElement('button');dock.textContent='Dock';dock.hidden=true;dock.setAttribute('aria-label','Dock project sidebar');browserHead.append(dock);dock.onclick=()=>{document.body.dataset.browserFloating='false';browserPanel.style.cssText='';dock.hidden=true;draw()};
window.addEventListener('resize',()=>{if(innerWidth<700&&document.body.dataset.browserFloating==='true')dock.click()});
let browserDrag=null;browserHead.onpointerdown=e=>{if(innerWidth<700||e.target.closest('button')||e.button!==0)return;const r=browserPanel.getBoundingClientRect();browserDrag={dx:e.clientX-r.left,dy:e.clientY-r.top};document.body.dataset.browserFloating='true';Object.assign(browserPanel.style,{left:`${r.left}px`,top:`${r.top}px`,width:`${r.width}px`,height:`${r.height}px`});dock.hidden=false;browserHead.setPointerCapture(e.pointerId);e.preventDefault();draw()};browserHead.onpointermove=e=>{if(!browserDrag)return;browserPanel.style.left=`${Math.max(0,Math.min(innerWidth-browserPanel.offsetWidth,e.clientX-browserDrag.dx))}px`;browserPanel.style.top=`${Math.max(0,Math.min(innerHeight-80,e.clientY-browserDrag.dy))}px`};browserHead.onpointerup=e=>{browserDrag=null;if(browserHead.hasPointerCapture(e.pointerId))browserHead.releasePointerCapture(e.pointerId)};
function showWorkspace(){document.body.dataset.screen='workbench';try{sessionStorage.setItem('oc-screen','workbench')}catch{};draw()}
function showHome(){$('resume-workspace').hidden=false;clearStockRequests();shell?.closeAll();document.body.dataset.screen='home';try{sessionStorage.setItem('oc-screen','home')}catch{};$('resume-name').textContent=project?`Current workspace: ${project.name}`:'';pausePlayback();window.scrollTo({top:0,behavior:'instant'})}
function renderHome(){
 $('process-cards').replaceChildren();$('new-strategy').replaceChildren();for(const strategy of catalog.strategies||[])$('new-strategy').add(new Option(strategy.label,strategy.id));
 for(const process of catalog.processes){const button=document.createElement('button');button.className='process-card'+(process.ready?'':' planned');const name=document.createElement('strong'),description=document.createElement('span'),availability=document.createElement('span');name.textContent=process.label;description.textContent=process.description;availability.className='availability';availability.textContent=process.ready?'2.5D + upper-surface 3D machining':'Reference workspace · motion engine planned';button.append(name,description,availability);button.dataset.process=process.id;button.onclick=()=>task(async()=>{if(project&&(project.part||canUndo)&&!await confirmAction({title:'New machining workspace?',message:'Start a new workspace? Save the current project first if you need a copy.',accept:'New workspace',destructive:true}))return;await sessionEdit({kind:'new_project',process:process.id});showWorkspace();setPane('import');viewport.setView(shell.prefs().defaultView);status(`${process.label} workspace opened.${process.ready?' Import a part to begin.':' Import, view and save models; machining strategies are planned.'}`)});$('process-cards').append(button)}shell?.homeIcons();
}
document.addEventListener('keydown',event=>{
 if(busy||document.body.dataset.screen!=='workbench'||event.target.closest('input,textarea,select,[contenteditable=true]')||document.querySelector('.file-menu').open||document.querySelector('#project-context-menu')||document.querySelector('dialog[open]'))return;
 const command=event.ctrlKey||event.metaKey,key=event.key.toLowerCase();let action;
 if(command&&key==='s')action=()=>$('save').click();
 else if(command&&key==='z')action=()=>(event.shiftKey?$('redo'):$('undo')).click();
 else if(command&&key==='y')action=()=>$('redo').click();
 else if(project.operations[selected]){
  if(command&&key==='d')action=()=>task(()=>sessionEdit({kind:'duplicate_operation',index:selected}));
  else if(key==='delete')action=()=>task(()=>sessionEdit({kind:'delete_operation',index:selected}));
  else if(event.altKey&&key==='arrowup'&&selected>0)action=()=>task(()=>sessionEdit({kind:'move_operation',index:selected,target:selected-1}));
  else if(event.altKey&&key==='arrowdown'&&selected<project.operations.length-1)action=()=>task(()=>sessionEdit({kind:'move_operation',index:selected,target:selected+1}));
  else if(key==='f2')action=()=>{shell.openOperation(selected)};
 }
 if(action){event.preventDefault();action()}
});
$('home-button').onclick=showHome;$('resume-workspace').onclick=showWorkspace;$('home-open').onclick=()=>$('open').click();
function groups(root){const open=new Set([...root.querySelectorAll('details[open]')].map(el=>el.querySelector('summary')?.textContent));root.replaceChildren();return title=>setupGroup(root,title,open.has(title))}
function basicFields(root,obj,labels={}){
 for(const [key,value] of Object.entries(obj)){
  const label=labels[key]||key.replaceAll('_',' ').replace(/^./,c=>c.toUpperCase());
  if(Array.isArray(value)&&value.every(v=>typeof v==='number'||v===null)){for(let i=0;i<value.length;i++)field(root,`${label} ${'XYZ'[i]||i+1}`,value,i,value[i]===null||key==='home'?'optional':'number')}
  else if(value&&typeof value==='object'&&!Array.isArray(value)){const box=setupGroup(root,label,false);basicFields(box,value)}
  else if(typeof value==='boolean')field(root,label,obj,key,'checkbox');
  else if(typeof value==='number'||value===null)field(root,label,obj,key,value===null||['maximum_load_kg','spindle_power_kw'].includes(key)?'optional':'number');
  else if(typeof value==='string')field(root,label,obj,key,'text');
 }
}
function renderExtras(){
 const process=catalog.processes.find(p=>p.id===project.process);$('process-label').textContent=`${process?.label||project.process} workspace`;$('resume-name').textContent=`Current workspace: ${project.name}`;
 $('process-status').textContent=process?.ready?'Choose model-based planning, recognized features or manual operations.':'This process supports reference models and project storage. Its motion engine is planned.';
 const s=project.stock;$('stock-summary').textContent=project.part?`Current stock: ${s.width} × ${s.height} × ${s.thickness} mm. Stock top is Z=0. Fit stock aligns the part inside this blank.`:'Import a part to fit stock automatically, or enter blank dimensions below.';
 $('part-settings').replaceChildren();if(project.part){field($('part-settings'),'Show part',project.part,'visible','checkbox');field($('part-settings'),'Source units',project.part,'units','text',[['millimetres','Millimetres'],['inches','Inches']])}
 const machineRoot=$('machine-setup');if(project.machine){const manual=setupGroup(machineRoot,'Tool changes',true),changerData=project.machine.tool_changer;
 const mode=field(manual,'Tool changing',changerData,'mode','text',[['mounted_tool','Mounted tool / manual changes'],['manual','Manual multi-tool · operator pause'],['milling_atc','Controller-managed ATC'],['turning_turret','Turning turret']]);
 const note=document.createElement('p');note.className='info-note form-wide';note.textContent='Multi-tool planning needs no ATC. For one multi-tool NC program, set a change position and calibrated H length registers in the tool library. Mach3/Generic use M0; LinuxCNC uses its configured manual M6 handshake. Start manual programs with the first tool mounted. A configured ATC station is the M6 entry/return point; the controller macro must return there. GRBL requires a separate program per tool.';manual.append(note);
 field(manual,'Tool-change time estimate · s',changerData,'change_seconds');if(changerData.mode==='milling_atc'){field(manual,'ATC magazine capacity',changerData,'capacity','integer')}if(changerData.manual_position){for(let i=0;i<3;i++)field(manual,`Tool change ${'XYZ'[i]} · work mm`,changerData.manual_position,i)}else{const set=document.createElement('button');set.textContent='Set tool-change position…';set.onclick=()=>task(async()=>{changerData.manual_position=[project.stock.width/2,project.stock.height/2,project.clearance+20];if(changerData.mode!=='milling_atc')changerData.mode='manual';await changed();render()});manual.append(set)}
 field(manual,'Controller M6 / LinuxCNC manual handshake configured',changerData,'controller_managed','checkbox');
 const advanced=setupGroup(machineRoot,'Machine configuration & tool changer',false);basicFields(setupGroup(advanced,'Configuration',false),project.machine.configuration,{axis_feed:'Axis maximum feed · mm/min',axis_acceleration:'Axis acceleration · mm/s²',home:'Home · mm',table_size:'Table size · mm',minimum_rpm:'Minimum spindle · RPM',maximum_load_kg:'Maximum load · kg',spindle_power_kw:'Spindle power · kW'});const legacy=setupGroup(advanced,'Machine tool geometry',false);for(const [key,value] of Object.entries(project.machine.tooling)){if(key==='type')continue;if(key==='kind')field(legacy,'Cutter kind',project.machine.tooling,key,'text',[['end_mill','End mill'],['drill','Drill']]);else {const unit=key.includes('temperature')?'°C':key.includes('rpm')?'RPM':key.includes('angle')?'°':key.includes('watts')?'W':key.includes('amps')?'A':'mm';field(legacy,key.replaceAll('_',' ')+' · '+unit,project.machine.tooling,key,typeof value==='number'?'number':'text')}}}
 renderLibrary();historyButtons();
}
let toolSearch='',toolScene=null,toolUnits='mm',toolDimensionKey='diameter';
const toolViewport=new Viewport($('tool-canvas'),()=>drawToolPreview(),()=>{});
new ResizeObserver(()=>drawToolPreview()).observe($('tool-canvas'));
function drawToolPreview(){if(!toolScene)return;if(toolViewport.controls.dark!==viewport.controls.dark){toolViewport.controls={...viewport.controls};toolViewport.scene=null;}const dimension=toolScene.dimensions?.find(d=>d.key===toolDimensionKey);toolViewport.annotationPoints=dimension?[dimension.start,dimension.end]:[];const view=toolViewport.draw(toolScene.project,toolScene.workspace,null,100,null,null,false,false);drawToolDimension(dimension,view)}
function drawToolDimension(dimension,view){
 const root=$('tool-dimensions');root.replaceChildren();if(!dimension||!view){$('tool-dimension-caption').textContent='Select a tool diameter or length to see its dimension.';return}
 const width=$('tool-canvas').clientWidth,height=$('tool-canvas').clientHeight;root.setAttribute('viewBox',`0 0 ${width} ${height}`);
 const a=view.projectPoint(dimension.start),b=view.projectPoint(dimension.end),length=Math.hypot(b[0]-a[0],b[1]-a[1]);if(length<1)return;
 const perpendicular=[-(b[1]-a[1])/length,(b[0]-a[0])/length],offset=38,p=[a[0]+perpendicular[0]*offset,a[1]+perpendicular[1]*offset],q=[b[0]+perpendicular[0]*offset,b[1]+perpendicular[1]*offset];
 const svg=(tag,attributes)=>{const node=document.createElementNS('http://www.w3.org/2000/svg',tag);for(const [key,value]of Object.entries(attributes))node.setAttribute(key,value);return node};
 const defs=svg('defs',{}),marker=svg('marker',{id:'dimension-arrow',viewBox:'0 0 10 10',refX:5,refY:5,markerWidth:7,markerHeight:7,orient:'auto-start-reverse'});marker.append(svg('path',{d:'M 10 0 L 0 5 L 10 10',fill:'none',stroke:'currentColor','stroke-width':1.5}));defs.append(marker);root.append(defs);
 for(const [from,to]of [[a,p],[b,q]])root.append(svg('line',{x1:from[0],y1:from[1],x2:to[0],y2:to[1],class:'dimension-extension'}));root.append(svg('line',{x1:p[0],y1:p[1],x2:q[0],y2:q[1],'marker-start':'url(#dimension-arrow)','marker-end':'url(#dimension-arrow)',class:'dimension-main'}));
 const value=Number((dimension.value_mm/(toolUnits==='in'?25.4:1)).toFixed(4)),label=`${dimension.key.includes('diameter')?'Ø ':''}${value} ${toolUnits==='in'?'in':'mm'}`;const text=svg('text',{x:Math.max(35,Math.min(width-35,(p[0]+q[0])/2+perpendicular[0]*13)),y:Math.max(15,Math.min(height-8,(p[1]+q[1])/2+perpendicular[1]*13)),'text-anchor':'middle',class:'dimension-value'});text.textContent=label;root.append(text);$('tool-dimension-caption').textContent=`${dimension.label}: ${label}`;
}

async function loadToolPreview(id){const rev=revision;try{const preview=await call('tool-preview',{tool_id:id}),meshes=preview.meshes;if(revision!==rev||toolFocus!==id)return;const vertices=meshes.flatMap(m=>m.vertices),min=[0,0,0],max=[0,0,0];for(const v of vertices)for(let i=0;i<3;i++){min[i]=Math.min(min[i],v[i]);max[i]=Math.max(max[i],v[i])}const offset=[-min[0],-min[1],-max[2]],transform=v=>v.map((x,i)=>x+offset[i]);$('tool-preview-note').textContent='Illustrative tool and holder geometry · drag to orbit, scroll or pinch to zoom. This is not collision geometry.';toolScene={dimensions:preview.dimensions.map(d=>({...d,start:transform(d.start),end:transform(d.end)})),project:{stock:{width:max[0]-min[0],height:max[1]-min[1],thickness:max[2]-min[2]},part:{visible:true,units:'millimetres',offset,geometry:{mesh:meshes[0]}}},workspace:[{...meshes[1],kind:'fixture',vertices:meshes[1].vertices.map(transform)}]};drawToolPreview()}catch(error){$('tool-preview-note').textContent=`Tool preview unavailable: ${error.message}. Review the current definition.`}}
$('library-editor').addEventListener('toggle',()=>{if($('library-editor').open)renderLibrary()});
function renderLibrary(){
 const root=$('tool-library-editor'),open=new Set([...root.querySelectorAll('details[open]')].map(el=>el.querySelector('summary')?.textContent));root.replaceChildren();
 const unitLabel=document.createElement('label');unitLabel.textContent='Tool display units';const unitSelect=document.createElement('select');unitSelect.setAttribute('aria-label','Tool display units');unitSelect.add(new Option('Millimetres','mm'));unitSelect.add(new Option('Inches','in'));unitSelect.value=toolUnits;unitSelect.onchange=()=>{toolUnits=unitSelect.value;renderLibrary()};unitLabel.append(unitSelect);root.append(unitLabel);
 const parameterField=(parent,parameter,object,dimension=false)=>{let unit=parameter.unit,factor=1;if(toolUnits==='in'){const converted={'mm':['in',25.4],'mm/min':['in/min',25.4],'mm/s':['in/s',25.4],'mm3/s':['in3/s',25.4**3]}[unit];if(converted)[unit,factor]=converted}const input=field(parent,`${parameter.label} · ${unit}`,object,parameter.key,'number',undefined,factor);if(!dimension&&['ramp_spindle_rpm','transition_feed'].includes(parameter.key)){input.disabled=true;const note=document.createElement('small');note.className='note';note.textContent=parameter.key==='ramp_spindle_rpm'?'Stored for library portability. Entry uses the operation spindle speed.':'Stored for library portability. Links use the operation cutting feed.';input.parentElement.append(note);}if(dimension){if(Number.isFinite(input.valueAsNumber))input.value=Number(input.valueAsNumber.toFixed(6));input.dataset.dimension=parameter.key;input.addEventListener('focus',()=>{toolDimensionKey=parameter.key;toolViewport.setView('front');drawToolPreview()});input.addEventListener('pointerenter',()=>{if(!document.activeElement?.matches('[data-dimension]')){toolDimensionKey=parameter.key;drawToolPreview()}})}};
 const actions=document.createElement('div');actions.className='library-actions';const kind=document.createElement('select');kind.setAttribute('aria-label','New tool type');for(const spec of catalog.tools)kind.add(new Option(`${spec.group} · ${spec.label}`,spec.id));kind.value='flat_end_mill';actions.append(kind);const add=document.createElement('button');add.textContent='Add tool';add.onclick=()=>task(async()=>{await sessionEdit({kind:'add_library_tool',tool_kind:kind.value});toolFocus=project.tool_library.tools.at(-1)?.id;renderLibrary()});actions.append(add);const save=document.createElement('button');save.textContent='Save tool library';save.onclick=()=>task(async()=>{download(await call('tool-library'),'tools.cam-tools.json');status('Tool library downloaded')});actions.append(save);root.append(actions);
 const search=document.createElement('input');search.type='search';search.placeholder='Search name, type or vendor';search.setAttribute('aria-label','Search tool library');search.value=toolSearch;root.append(search);const choose=document.createElement('select');choose.setAttribute('aria-label','Edit library tool');root.append(choose);const body=document.createElement('div');root.append(body);
 const selectTools=()=>{choose.replaceChildren();const matches=project.tool_library.tools.filter(t=>`${t.name} ${t.kind} ${t.vendor}`.toLowerCase().includes(toolSearch.toLowerCase()));for(const tool of matches)choose.add(new Option(`T${tool.post.number} · ${tool.name}`,String(tool.id)));if(matches.some(t=>t.id===(toolFocus??project.active_tool)))choose.value=String(toolFocus??project.active_tool);toolFocus=choose.value?Number(choose.value):null;renderTool()};
 const renderTool=()=>{
  body.replaceChildren();const tool=project.tool_library.tools.find(t=>t.id===toolFocus);if(!tool){const p=document.createElement('p');p.textContent='No matching tools. Add a definition above or import a tool library.';body.append(p);$('tool-canvas').hidden=true;return}$('tool-canvas').hidden=false;const group=title=>setupGroup(body,title,open.has(title));const spec=catalog.tools.find(s=>s.id===tool.kind);if($('library-editor').open)loadToolPreview(tool.id);
  field(body,'Library tool name',tool,'name','text');const note=document.createElement('p');note.textContent=`${spec?.label||tool.kind} · ${tool.presets.length} cutting presets`;body.append(note);
  let box=group('Tool identity & workspace use');for(const key of ['description','vendor','product_id','product_link','material'])field(box,key.replaceAll('_',' '),tool,key,'text');field(box,'Flutes',tool,'flutes','integer');const spin=field(box,'Spindle rotation',tool,'clockwise','text',[['cw','Clockwise · M3'],['ccw','Counterclockwise · M4']]);spin.value=tool.clockwise?'cw':'ccw';spin.oninput=spin.onchange=()=>{tool.clockwise=spin.value==='cw';markFields()};field(box,'Center cutting',tool,'center_cutting','checkbox');for(const process of catalog.processes){const label=document.createElement('label');label.className='inline-check';const input=document.createElement('input');input.type='checkbox';input.checked=tool.uses.includes(process.id);input.onchange=()=>task(async()=>{tool.uses=input.checked?[...new Set([...tool.uses,process.id])]:tool.uses.filter(id=>id!==process.id);await changed();render()});label.append(input,document.createTextNode(process.label));box.append(label)}
  box=group('Tool geometry');for(const parameter of spec?.geometry||[])parameterField(box,parameter,tool.geometry,true);field(box,'Holder',tool,'holder','id',[['','No holder'],...project.tool_library.tools.filter(t=>t.kind==='holder'&&t.id!==tool.id).map(t=>[String(t.id),t.name])]);
  box=group('Shaft sections');tool.shaft.forEach((section,index)=>{const sectionBox=setupGroup(box,`Section ${index+1}`,false);basicFields(sectionBox,section);const remove=document.createElement('button');remove.textContent='Remove shaft section';remove.onclick=()=>task(async()=>{tool.shaft.splice(index,1);await changed();render()});sectionBox.append(remove)});const shaft=document.createElement('button');shaft.textContent='Add shaft section';shaft.onclick=()=>task(async()=>{tool.shaft.push({height:10,lower_diameter:tool.geometry.shank_diameter||tool.geometry.diameter||6,upper_diameter:tool.geometry.shank_diameter||tool.geometry.diameter||6});await changed();render()});box.append(shaft);
  box=group('Cutting presets');field(box,'Default cutting preset',tool,'default_preset','id',tool.presets.map(p=>[String(p.id),p.name]));for(const preset of tool.presets){const fields=setupGroup(box,preset.name,false);field(fields,'Preset name',preset,'name','text');field(fields,'Preset material',preset,'material','text');field(fields,'Feed mode',preset,'feed_mode','text',[['per_minute','Per minute'],['per_tooth','Per tooth'],['per_revolution','Per revolution']]);field(fields,'Preset coolant',preset,'coolant','text',[['off','Off'],['flood','Flood']]);field(fields,'Constant surface speed',preset,'constant_surface_speed','checkbox');for(const parameter of spec?.cutting||[])parameterField(fields,parameter,preset.values);field(fields,'Relative extrusion',preset,'relative_extrusion','checkbox');for(const strategy of catalog.strategies||[]){const label=document.createElement('label');label.className='inline-check';const checkbox=document.createElement('input');checkbox.type='checkbox';checkbox.checked=preset.strategies.includes(strategy.id);checkbox.onchange=()=>task(async()=>{preset.strategies=checkbox.checked?[...new Set([...preset.strategies,strategy.id])]:preset.strategies.filter(id=>id!==strategy.id);await changed();render()});label.append(checkbox,document.createTextNode(strategy.label));fields.append(label)}const remove=document.createElement('button');remove.textContent='Delete this preset';remove.onclick=()=>task(()=>sessionEdit({kind:'remove_preset',tool_id:tool.id,preset_id:preset.id}));fields.append(remove)}
  const addPreset=document.createElement('button');addPreset.textContent='Duplicate default preset';addPreset.onclick=()=>task(()=>sessionEdit({kind:'add_preset',tool_id:tool.id}));box.append(addPreset);
  box=group('Tool postprocessor settings');for(const [key,value] of Object.entries(tool.post)){if(Array.isArray(value)){for(let i=0;i<value.length;i++)field(box,`Mount offset ${'XYZ'[i]} · mm`,value,i)}else field(box,key.replaceAll('_',' '),tool.post,key,typeof value==='boolean'?'checkbox':typeof value==='string'?'text':value===null||['turning_offset','length_offset','diameter_offset','pocket'].includes(key)?'optional-integer':'number')}
  const buttons=document.createElement('div');buttons.className='library-actions';for(const [label,action] of [['Use this tool',null],['Duplicate tool','duplicate_tool'],['Delete tool','remove_tool']]){const b=document.createElement('button');b.textContent=label;b.onclick=()=>task(async()=>{if(action){await sessionEdit({kind:action,id:tool.id});toolFocus=project.active_tool;renderLibrary()}else{project.active_tool=tool.id;project.active_preset=tool.default_preset;await changed();render();status('Active tool and default cutting preset selected')}});buttons.append(b)}body.append(buttons);
 };
 choose.onchange=()=>{toolFocus=Number(choose.value);renderTool()};search.oninput=()=>{toolSearch=search.value;selectTools()};selectTools();
}

$('json').oninput=()=>{$('json').dataset.dirty='true'};
$('advanced-editor').addEventListener('toggle',()=>{if($('json').dataset.dirty!=='true')refreshEditor()});
$('version').textContent=`Build ${BUILD_ID}`;
$('undo').onclick=()=>task(()=>sessionEdit({kind:'undo'}));
$('redo').onclick=()=>task(()=>sessionEdit({kind:'redo'}));
$('backup').onclick=()=>download(projectJSON(),filename('.draft.json'));
$('recover').onclick=()=>task(async()=>{const data=await readSnapshot('previous');if(!data)throw Error('No previous autosave is available');const p=await call('draft',{project:data});if(await confirmAction({title:'Restore previous autosave?',message:'The current workspace remains available through Undo.',accept:'Restore autosave'})){await adopt(p);status('Previous autosave restored')}});
window.addEventListener('beforeunload',e=>{if(!autosaved){e.preventDefault();e.returnValue=''}});
function flushFieldsOnLeave(){if(fieldsDirty&&!busy)void task(async()=>{})}
document.addEventListener('visibilitychange',()=>{if(document.visibilityState==='hidden')flushFieldsOnLeave()});
window.addEventListener('pagehide',flushFieldsOnLeave);
viseUI=initVise({project:()=>project,call,edit:sessionEdit,task,status,viewport,beginPick,treeChanged:()=>sceneUI?.render(),showTree:()=>{if(innerWidth>=1100)document.body.dataset.browser='true';else shell.hideBrowser()},clearComponentHighlight:()=>{highlight=null;draw()},showClampingStock:()=>{viewport.controls.clampingStock=true;viewport.scene=null;highlight=null;draw()},previewPose:(id,resume)=>{shell.hideBrowser();viewport.controls.clampingStock=true;viewport.scene=null;highlight=null;pickCancel=resume;$('pick-face').textContent='Back to vise setup';$('pick-face').hidden=false;$('selection-hint').hidden=false;$('selection-hint').textContent='Raw stock and geometric jaw contact · review supports · drag to orbit';draw()},clearHighlight:()=>{highlight=null;viewport.controls.clampingStock=false;viewport.scene=null;draw()},inspectComponent:(id,index,resume)=>{highlight={component:{model:id,body:index}};draw()},inspectBody:(id,resume)=>{shell.hideBrowser();const m=workspacePreview.find(m=>m.id===id);if(!m)return;highlight={movingModel:id};pickCancel=resume;$('pick-face').hidden=false;$('selection-hint').hidden=false;$('selection-hint').textContent='Highlighted jaw group will move together · press Cancel selection to return';draw()},highlight:face=>{highlight=face;draw()},closeAll:()=>{if(inspectionMode)toggleInspection(false);shell.closeAll()},download});
$('fit-vise').onclick=()=>sceneUI.centerVise();
sceneUI=initScene({project:()=>project,call,edit:sessionEdit,task,status,viewport,highlight:(id,body)=>{if(inspectionMode)toggleInspection(false);if(viewport.controls.clampingStock)cancelPicking();highlight={component:{model:id,body}};draw()},viseOpen:()=>viseUI.isOpen(),suspendVise:()=>viseUI.suspend(),editExampleVise:model=>shell.openVise(model),openVise:id=>{sceneUI.close();viseUI.open(id)},viseState:id=>viseUI.treeState(id),toggleBody:(id,body,checked)=>viseUI.toggleBody(id,body,checked)});
$('open-vise-library').onclick=()=>viseUI.showLibrary();
shell=initShell({project:()=>project,inspectionMode:()=>inspectionMode,inspection:()=>inspection,toggleInspection,sendToVise,catalog:()=>catalog,presentation:()=>presentation,selected:()=>selected,selectedOperations,select:(index,event)=>{selectOperation(index,event);if(inspectionMode){inspectionMode=false;inspection=null;document.body.dataset.featureInspect='false'}},busy:()=>busy,dirty:()=>fieldsDirty,viewport,draw,task,call,edit:sessionEdit,changed,render,status,context:showOperationMenu,operationActions,focusTool:id=>{toolFocus=id},renderLibrary,download,home:showHome,pickOperation,recognizeAll,cancelPicking,geometryState:()=>geometryState,geometryDraft,clearGeometrySelection});
$('machine-file').onchange=()=>task(async()=>{const file=$('machine-file').files[0];if(!file)return;$('machine-file').value='';await shell.importMachine(file);shell.openMachines()});
await task(async()=>{
 catalog=await call('catalog');await shell.restoreLibraries();renderHome();const loaded=await loadWorkspace(),saved=loaded.data;let failed=false;
 if(saved!==null){try{project=await call('draft',{project:saved}).catch(()=>call('normalize',{project:saved}))}catch{failed=true;$('recovery').hidden=false;$('raw-backup').onclick=()=>download(saved,'recovered-workspace.draft.json')}}
 if(!project)project=await call('blank');applyView(await call('session-start'));lastState=projectJSON();render();viewport.setView(shell.prefs().defaultView);showHome();$('resume-workspace').hidden=!saved||failed;$('resume-name').textContent=saved&&!failed?`Saved workspace: ${project.name}`:'';autosaved=!failed&&!loaded.error;
 status(failed?'Saved workspace could not be loaded. It has been preserved; download it using Recover draft.':loaded.error?`Ready · Autosave unavailable: ${loaded.error.message}. Download a backup before closing.`:'Ready · All machining calculations run on this device',failed||!!loaded.error);
 $('saved').textContent=failed?'Recovery required':loaded.error?'Not saved — download a backup':saved?'Autosave restored':'No project saved yet';$('saved').dataset.state=autosaved?'saved':'error';
});
$('settings-apply-update').onclick=()=>$('update').click();
$('check-updates').onclick=async()=>{
 const button=$('check-updates'),message=$('update-check-status');button.disabled=true;message.textContent='Checking for updates…';
 try{
  if(!('serviceWorker'in navigator))throw Error('Updates are unavailable in this browser.');
  if(!navigator.onLine)throw Error('You are offline. Connect to the internet to check for updates.');
  const reg=await navigator.serviceWorker.getRegistration();
  if(!reg)throw Error('Update service is not ready. Please retry shortly.');
  await reg.update();
  const installing=reg.installing;
  const show=()=>{const available=!!(reg.waiting&&reg.active);$('settings-apply-update').hidden=!available;message.textContent=available?'An update is ready to install.':installing?.state==='redundant'?'The update download failed. Please retry.':reg.installing?'An update is downloading.':'You are up to date.';};
  installing?.addEventListener('statechange',show);show();
 }catch(error){message.textContent=`Could not check for updates: ${error.message}`;}
 finally{button.disabled=false;}
};
if('serviceWorker'in navigator){
 let controlled=!!navigator.serviceWorker.controller,activating=false;
 navigator.serviceWorker.addEventListener('controllerchange',()=>{if(controlled&&!activating){location.reload();return}controlled=true;$('update').hidden=true});
 navigator.serviceWorker.register('./sw.js',{updateViaCache:'none'}).then(async reg=>{
  function offer(){if(reg.waiting&&reg.active){$('update').hidden=false;$('update').textContent='Update available · reload'}else if(!activating){$('update').hidden=true}}
  offer();reg.addEventListener('updatefound',()=>{reg.installing?.addEventListener('statechange',offer)});
  $('update').onclick=async()=>{
   if(busy){status('Finish the current operation before updating.',true);return}
   if(activating)return;
   if(fieldsDirty)await task(async()=>{});
   if(!await persist()){status('Download a backup before updating; autosave failed.',true);return}
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
