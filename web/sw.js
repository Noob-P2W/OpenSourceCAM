const CACHE='opensourcecam-__BUILD_ID__';
const ASSETS=['./','./index.html','./style.css','./app.js','./build.js','./worker.js','./manifest.webmanifest','./icon.svg','./icon-192.png','./icon-512.png','./pkg/opensourcecam.js','./pkg/opensourcecam_bg.wasm'];
self.addEventListener('install',event=>event.waitUntil(caches.open(CACHE).then(c=>c.addAll(ASSETS.map(url=>new Request(url,{cache:"reload"}))))));
self.addEventListener('activate',event=>event.waitUntil(caches.keys().then(keys=>Promise.all(keys.filter(k=>k.startsWith('opensourcecam-')&&k!==CACHE).map(k=>caches.delete(k)))).then(()=>self.clients.claim())));
self.addEventListener('fetch',event=>{if(event.request.method!=='GET'||new URL(event.request.url).origin!==location.origin)return;event.respondWith(caches.open(CACHE).then(async cache=>(await cache.match(event.request))||fetch(event.request)))});

self.addEventListener('message',event=>{if(event.data?.type==='ACTIVATE')self.skipWaiting()});
