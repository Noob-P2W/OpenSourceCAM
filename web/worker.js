import init, * as core from './pkg/opensourcecam.js';
const ready = init();
let session;
self.onmessage = async ({data: {id, action, project, file, inches, edit}}) => {
  try {
    await ready;
    let value;
    if(action==='session-start'){const next=new core.WebSession(project);session?.free();session=next;value=JSON.parse(session.view())}
    else if(action==='draft'){const draft=new core.WebSession(project);try{value=JSON.parse(draft.view()).project}finally{draft.free()}}
    else if(action==='session-edit') value=JSON.parse(session.edit(JSON.stringify(edit)));
    else if(action==='sample'||action==='operation') value=JSON.parse(core[action]());
    else if(action==='normalize') value=JSON.parse(core.normalize(project));
    else if(action==='import') value=JSON.parse(core.import_model(file.name,new Uint8Array(file.bytes),inches));
    else value=JSON.parse(session ? session.run(action) : core.run(project,action));
    self.postMessage({id,value});
  } catch(error) { self.postMessage({id,error:String(error)}); }
};
