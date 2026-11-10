const CACHE='opensourcecam-eaa97ba31b6a';
const ASSETS=['./cad-worker.js','./cad/occt-import-js.js','./cad/occt-import-js.wasm','./cad/license.occt-import-js.txt','./cad/license.occt.txt','./cad/NOTICES.txt','./cad/license.occt-exception.txt','./','./index.html','./style.css','./app.js','./shell.js','./dialogs.js','./assets/commands.svg','./viewport.js','./build.js','./worker.js','./manifest.webmanifest','./icon.svg','./icon-192.png','./icon-512.png','./pkg/opensourcecam.js','./pkg/opensourcecam_bg.wasm'];
self.addEventListener('install',event=>event.waitUntil((async()=>{
 const cache=await caches.open(CACHE);
 await Promise.all(ASSETS.map(async path=>{
  const url=new URL(path,self.location.href);url.searchParams.set('build',CACHE);
  const response=await fetch(url,{cache:'no-store'});
  if(!response.ok)throw Error(`Cannot cache ${path}: HTTP ${response.status}`);
  await cache.put(path,response);
 }));
})()));
self.addEventListener('activate',event=>event.waitUntil((async()=>{
 const keys=(await caches.keys()).filter(key=>key.startsWith('opensourcecam-'));
 const current=keys.indexOf(CACHE);
 // Never delete a newer waiting worker's completed cache during an older activation.
 // Retain the preceding release too, while clients switch to this worker.
 await Promise.all(keys.slice(0,Math.max(0,current-1)).map(key=>caches.delete(key)));
 await self.clients.claim();
})()));
self.addEventListener('fetch',event=>{if(event.request.method!=='GET'||new URL(event.request.url).origin!==location.origin)return;event.respondWith(caches.open(CACHE).then(async cache=>(await cache.match(event.request))||fetch(event.request)))});

self.addEventListener('message',event=>{if(event.data?.type==='ACTIVATE')event.waitUntil(self.skipWaiting())});
