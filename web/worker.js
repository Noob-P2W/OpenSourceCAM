import init, * as core from './pkg/opensourcecam.js';
const ready = init();
self.onmessage = async ({data: {id, action, project, file, inches}}) => {
  try {
    await ready;
    let value;
    if(action==='sample'||action==='operation') value=JSON.parse(core[action]());
    else if(action==='normalize') value=JSON.parse(core.normalize(project));
    else if(action==='import') value=JSON.parse(core.import_model(file.name,new Uint8Array(file.bytes),inches));
    else value=JSON.parse(core.run(project,action));
    self.postMessage({id,value});
  } catch(error) { self.postMessage({id,error:String(error)}); }
};
