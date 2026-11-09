const CACHE='opensourcecam-v1';
const ASSETS=['./','./index.html','./style.css','./app.js','./worker.js','./manifest.webmanifest','./icon.svg','./icon-192.png','./icon-512.png','./pkg/opensourcecam.js','./pkg/opensourcecam_bg.wasm'];
self.addEventListener('install',event=>event.waitUntil(caches.open(CACHE).then(c=>c.addAll(ASSETS))));
self.addEventListener('activate',event=>event.waitUntil(caches.keys().then(keys=>Promise.all(keys.filter(k=>k.startsWith('opensourcecam-')&&k!==CACHE).map(k=>caches.delete(k))))));
self.addEventListener('fetch',event=>{if(event.request.method!=='GET'||new URL(event.request.url).origin!==location.origin)return;event.respondWith(caches.open(CACHE).then(async cache=>(await cache.match(event.request))||fetch(event.request)))});
