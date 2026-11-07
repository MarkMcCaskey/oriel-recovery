const VERSION='oriel-__BUILD__';
const ASSETS=['./','index.html','style.css','app.js','worker.js','core.wasm','icon.svg','manifest.webmanifest','README.txt'];
self.addEventListener('install',event=>event.waitUntil(caches.open(VERSION).then(cache=>cache.addAll(ASSETS)).then(()=>self.skipWaiting())));
self.addEventListener('activate',event=>event.waitUntil(caches.keys().then(keys=>Promise.all(keys.filter(k=>k.startsWith('oriel-')&&k!==VERSION).map(k=>caches.delete(k)))).then(()=>self.clients.claim())));
self.addEventListener('fetch',event=>{if(event.request.method!=='GET'||new URL(event.request.url).origin!==location.origin)return;event.respondWith(caches.open(VERSION).then(async cache=>(await cache.match(event.request))||fetch(event.request)));});
