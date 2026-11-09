import init, * as core from './pkg/opensourcecam.js';
const ready = init();
let session;
function importCad(file,id){return new Promise((resolve,reject)=>{
 const worker=new Worker(new URL('./cad-worker.js',import.meta.url));
 const timer=setTimeout(()=>{worker.terminate();reject(Error('CAD import exceeded 120 seconds. Try a simpler model.'))},120000);
 function finish(error,value){clearTimeout(timer);worker.terminate();error?reject(error):resolve(value)}
 worker.onerror=()=>finish(Error('CAD worker failed. This model may exceed available device memory.'));
 worker.onmessage=({data})=>{if(data.progress){self.postMessage({id,progress:data.progress});return}if(data.error){finish(Error(data.error));return}try{const source=new TextDecoder('utf-8',{fatal:true}).decode(file.bytes);finish(null,JSON.parse(core.cad_project(file.name,source,data.format,JSON.stringify(data.mesh))))}catch(error){finish(error)}};
 worker.postMessage(file);
})}
self.onmessage = async ({data: {id, action, project, file, inches, edit, role, options, planner, progress, resolution, tool_id, history}}) => {
  try {
    await ready;
    let value;
    if(action==='catalog') value=JSON.parse(core.catalog());
    else if(action==='plan-model') value=JSON.parse(session.plan(planner,JSON.stringify(options)));
    else if(action==='tool-preview') value=JSON.parse(session.tool_preview(BigInt(tool_id)));
    else if(action==='simulate-current') value=JSON.parse(session.simulate(progress,resolution));
    else if(action==='stock-obj') value=session.stock_obj(progress,resolution);
    else if(action==='machine-profile') value=session.machine_profile();
    else if(action==='tool-library') value=session.tool_library();
    else if(action==='history-checkpoint') value=session.history_checkpoint();
    else if(action==='restore-session'){const next=new core.WebSession(project);try{value=JSON.parse(next.restore_history(history))}catch(error){next.free();throw error}session?.free();session=next}
    else if(action==='session-start'){const next=new core.WebSession(project);session?.free();session=next;value=JSON.parse(session.view())}
    else if(action==='draft'){const draft=new core.WebSession(project);try{value=JSON.parse(draft.view()).project}finally{draft.free()}}
    else if(action==='session-edit') value=JSON.parse(session.edit(JSON.stringify(edit)));
    else if(action==='sample'||action==='operation') value=JSON.parse(core[action]());
    else if(action==='save') value=session.save_project();
    else if(action==='normalize') value=JSON.parse(core.normalize(project));
    else if(action==='import'){
     if(/\.(step|stp|iges|igs)$/i.test(file.name)){
      self.postMessage({id,progress:'Loading local CAD kernel…'});const imported=await importCad(file,id);
      value=JSON.parse(session.edit(JSON.stringify(role==='part'?{kind:'import_part',part:imported.part}:{kind:'add_workspace',part:imported.part,model_kind:role})));
     }else value=JSON.parse(session.import_file(file.name,new Uint8Array(file.bytes),inches,role||'part'));
    }
    else value=JSON.parse(session ? session.run(action) : core.run(project,action));
    self.postMessage({id,value});
  } catch(error) { self.postMessage({id,error:String(error)}); }
};
