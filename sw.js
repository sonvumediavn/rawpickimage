/* Service worker: cho app mở được khi mất mạng và lưu tạm ô bản đồ đã xem. */
const VERSION = 'dch-v1';
const SHELL = [
  './', 'index.html', 'css/style.css',
  'js/icons.js', 'js/db.js', 'js/demo.js', 'js/app.js',
  'vendor/leaflet/leaflet.js', 'vendor/leaflet/leaflet.css',
  'icons/icon.svg', 'icons/icon-192.png', 'icons/icon-512.png', 'manifest.webmanifest'
];
const TILE_CACHE = 'dch-tiles';
const TILE_MAX = 1500;
const TILE_HOSTS = ['basemaps.cartocdn.com', 'tile.openstreetmap.org', 'server.arcgisonline.com'];

self.addEventListener('install', e => {
  e.waitUntil(caches.open(VERSION).then(c => c.addAll(SHELL)).then(() => self.skipWaiting()));
});

self.addEventListener('activate', e => {
  e.waitUntil(
    caches.keys()
      .then(keys => Promise.all(keys.filter(k => k !== VERSION && k !== TILE_CACHE).map(k => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

async function trimTiles() {
  const c = await caches.open(TILE_CACHE);
  const keys = await c.keys();
  for (let i = 0; i < keys.length - TILE_MAX; i++) await c.delete(keys[i]);
}

self.addEventListener('fetch', e => {
  const req = e.request;
  if (req.method !== 'GET') return;
  const url = new URL(req.url);

  // Ô bản đồ: lấy từ cache trước, cập nhật nền.
  if (TILE_HOSTS.some(h => url.hostname.endsWith(h))) {
    e.respondWith(caches.open(TILE_CACHE).then(async c => {
      const hit = await c.match(req);
      const net = fetch(req).then(res => {
        if (res.ok || res.type === 'opaque') { c.put(req, res.clone()); trimTiles(); }
        return res;
      }).catch(() => hit);
      return hit || net;
    }));
    return;
  }

  // Tệp của app: mạng trước để luôn có bản mới, mất mạng thì dùng cache.
  if (url.origin === location.origin) {
    e.respondWith(
      fetch(req).then(res => {
        if (res.ok) caches.open(VERSION).then(c => c.put(req, res.clone()));
        return res;
      }).catch(() => caches.match(req, { ignoreSearch: true }).then(r => r || caches.match('index.html')))
    );
  }
});
