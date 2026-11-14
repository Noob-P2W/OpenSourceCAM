// Device-local project snapshots. Current and previous always commit together.
const DATABASE='opensourcecam-workspace',STORE='snapshots';
let connection=null,writes=Promise.resolve();
function database(){
 if(!connection)connection=new Promise((resolve,reject)=>{
  const request=indexedDB.open(DATABASE,1);let blocked=false;
  request.onupgradeneeded=()=>request.result.createObjectStore(STORE);
  request.onerror=()=>{connection=null;reject(request.error)};
  request.onblocked=()=>{blocked=true;connection=null;reject(Error('Workspace storage is blocked by another tab. Close the other tab and retry.'))};
  request.onsuccess=()=>{const db=request.result;if(blocked){db.close();return}db.onversionchange=()=>{db.close();connection=null};resolve(db)};
 }).catch(error=>{connection=null;throw error});
 return connection;
}
async function transaction(mode,action){
 const db=await database();return new Promise((resolve,reject)=>{
  const tx=db.transaction(STORE,mode);let value,failure;
  const abort=error=>{failure=error;tx.abort()};
  tx.oncomplete=()=>resolve(value);tx.onabort=()=>reject(failure||tx.error||Error('Workspace save was interrupted.'));
  try{action(tx.objectStore(STORE),result=>{value=result},abort)}catch(error){abort(error)}
 });
}
export async function readSnapshot(key='current'){
 await writes;return transaction('readonly',(store,set,abort)=>{
  const request=store.get(key);request.onsuccess=()=>{const row=request.result;if(row!==undefined&&(!row||typeof row.data!=='string')){abort(Error('Saved workspace record is damaged.'));return}set(row?.data??null)};
 });
}
function legacy(key){try{return localStorage.getItem(key)}catch{return null}}
export async function loadWorkspace(){
 try{
  const current=await readSnapshot();if(current!==null)return{data:current,error:null};
  const old=legacy('oc-project'),previous=legacy('oc-backup');if(old===null&&previous===null)return{data:null,error:null};
  let migratedPrevious=false;
  const data=await transaction('readwrite',(store,set,abort)=>{
   const request=store.get('current');request.onsuccess=()=>{try{
    if(request.result){set(request.result.data);return}
    if(old!==null)store.put({data:old,savedAt:Date.now()},'current');
    if(previous!==null){const backup=store.get('previous');backup.onsuccess=()=>{try{if(!backup.result)store.put({data:previous,savedAt:Date.now()},'previous');migratedPrevious=!backup.result||backup.result.data===previous}catch(error){abort(error)}}}
    set(old);
   }catch(error){abort(error)}};
  });
  // Remove legacy text only after its IndexedDB transaction has committed.
  if(data===old)try{if(legacy('oc-project')===old)localStorage.removeItem('oc-project');if(migratedPrevious&&legacy('oc-backup')===previous)localStorage.removeItem('oc-backup')}catch{}
  return{data,error:null};
 }catch(error){return{data:legacy('oc-project'),error}}
}
export function saveWorkspace(data){
 if(typeof data!=='string')return Promise.reject(Error('Workspace snapshot must be serialized JSON.'));
 const saved=writes.then(()=>transaction('readwrite',(store,set,abort)=>{
  const request=store.get('current');request.onsuccess=()=>{try{
   const previous=request.result;if(previous?.data===data)return;
   if(previous)store.put(previous,'previous');
   store.put({data,savedAt:Date.now()},'current');
  }catch(error){abort(error)}};
 }));
 writes=saved.catch(()=>{});return saved;
}
