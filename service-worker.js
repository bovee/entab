self.addEventListener('install', evt => {
  evt.waitUntil(
    caches.open('entab').then(cache => {
      return cache.add('/index.html').then(() => self.skipWaiting());
    })
  )
});

self.addEventListener('activate', evt => {
  evt.waitUntil(self.clients.claim());
});

self.addEventListener("fetch", evt => {
  // fix for the bug here: https://bugs.chromium.org/p/chromium/issues/detail?id=823392
  if (evt.request.cache === "only-if-cached" && evt.request.mode !== "same-origin") {
    return
  }

  evt.respondWith(
    caches.match(evt.request).then(res => res || fetch(evt.request)),
  );
});
