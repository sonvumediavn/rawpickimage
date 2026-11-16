/* Dấu Chân Hồng — logic chính.
   Quy hoạch: Bản đồ (ở đâu) · Nhật ký (khi nào) · Địa điểm (nơi nào đáng nhớ) · Dấu chân (đã đi bao nhiêu). */
(() => {
  'use strict';

  /* ================= Hằng số ================= */
  const CATS = [
    { id: 'food', label: 'Quán ăn', e: '🍜' },
    { id: 'cafe', label: 'Cà phê', e: '☕' },
    { id: 'sight', label: 'Tham quan', e: '📸' },
    { id: 'nature', label: 'Thiên nhiên', e: '🌿' },
    { id: 'shop', label: 'Mua sắm', e: '🛍️' },
    { id: 'stay', label: 'Lưu trú', e: '🏨' },
    { id: 'fun', label: 'Vui chơi', e: '🎡' },
    { id: 'other', label: 'Khác', e: '📍' }
  ];
  const CAT = Object.fromEntries(CATS.map(c => [c.id, c]));
  const catOf = c => CAT[c.cat] || CAT.other;
  const MOODS = [
    { v: 5, e: '😍', l: 'Mê mẩn' },
    { v: 4, e: '😊', l: 'Vui' },
    { v: 3, e: '😌', l: 'Bình yên' },
    { v: 2, e: '😐', l: 'Tạm ổn' },
    { v: 1, e: '😢', l: 'Hơi buồn' }
  ];
  const MOOD = Object.fromEntries(MOODS.map(m => [m.v, m]));
  const TAGS = ['Đáng quay lại', 'Đồ ăn ngon', 'View đẹp', 'Giá hợp lý', 'Phục vụ tốt', 'Không gian xinh', 'Chụp ảnh đẹp', 'Yên tĩnh', 'Hơi đắt', 'Đông đúc', 'Khó tìm', 'Phục vụ chậm'];
  const PRICES = ['Miễn phí', '₫', '₫₫', '₫₫₫', '₫₫₫₫'];
  const WEEKDAYS = ['Chủ nhật', 'Thứ Hai', 'Thứ Ba', 'Thứ Tư', 'Thứ Năm', 'Thứ Sáu', 'Thứ Bảy'];
  const STRIDE_M = 0.7; // độ dài trung bình một bước chân

  // Ngưỡng zoom cho từng cấp hiển thị
  const Z_COUNTRY = 4.5;  // nhỏ hơn: gom theo quốc gia
  const Z_LOCAL = 8.5;    // từ đây: ghim từng địa điểm (dưới đó gom theo tỉnh/thành)
  const Z_DETAIL = 14.5;  // từ đây: ghim có ảnh và tên
  const Z_ROUTES = 10;    // từ đây: hiện các lộ trình đã đi
  const VN_VIEW = [[8.3, 101.8], [23.5, 111.2]];
  const ISLES = [
    { lat: 16.45, lng: 111.9, name: 'Q.đ. Hoàng Sa<br>(Việt Nam)' },
    { lat: 9.9, lng: 114.2, name: 'Q.đ. Trường Sa<br>(Việt Nam)' }
  ];

  const TILES = {
    voyager: { name: 'Bản đồ phố', url: 'https://{s}.basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}{r}.png', opt: { subdomains: 'abcd', maxZoom: 20 }, by: '© <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener">OpenStreetMap</a> · © <a href="https://carto.com/attributions" target="_blank" rel="noopener">CARTO</a>' },
    light: { name: 'Tối giản', url: 'https://{s}.basemaps.cartocdn.com/light_all/{z}/{x}/{y}{r}.png', opt: { subdomains: 'abcd', maxZoom: 20 }, by: '© <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener">OpenStreetMap</a> · © <a href="https://carto.com/attributions" target="_blank" rel="noopener">CARTO</a>' },
    sat: { name: 'Vệ tinh', url: 'https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}', opt: { maxZoom: 19 }, by: 'Ảnh vệ tinh © Esri' }
  };

  /* ================= Trạng thái ================= */
  const S = {
    view: 'map',
    journeys: [],
    checkins: [],
    rev: 0,               // tăng mỗi khi dữ liệu đổi, để biết lúc nào cần vẽ lại
    _places: null,
    active: null,         // hành trình đang ghi / tạm dừng
    me: null,
    watchId: null,
    follow: false,
    gpsMsg: '',
    gpsWarn: false,
    tile: 'voyager',
    focusId: null,
    diaryMode: 'timeline',
    diaryReg: 'all',
    diaryQuery: '',
    placeCat: 'all',
    placeQuery: '',
    placeSort: 'recent',
    draft: null,
    picking: false,
    sheetOpen: false,
    sheetHist: false,
    sheetKind: null,
    ignorePop: 0,
    wakeLock: null,
    lastSave: 0
  };

  /* ================= Tiện ích ================= */
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => [...r.querySelectorAll(s)];
  const uid = () => (crypto.randomUUID ? crypto.randomUUID() : Date.now().toString(36) + Math.random().toString(36).slice(2));
  const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  const pad = n => String(n).padStart(2, '0');
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const nf = new Intl.NumberFormat('vi-VN');
  const nf1 = new Intl.NumberFormat('vi-VN', { maximumFractionDigits: 1, minimumFractionDigits: 1 });

  function hav(a, b) {
    const R = 6371000, toR = Math.PI / 180;
    const dLat = (b[0] - a[0]) * toR, dLng = (b[1] - a[1]) * toR;
    const s = Math.sin(dLat / 2) ** 2 + Math.cos(a[0] * toR) * Math.cos(b[0] * toR) * Math.sin(dLng / 2) ** 2;
    return 2 * R * Math.asin(Math.sqrt(s));
  }
  const kmNum = m => (m / 1000 < 100 ? nf1.format(m / 1000) : nf.format(Math.round(m / 1000)));
  const km = m => (m < 1000 ? `${Math.round(m)} m` : `${kmNum(m)} km`);
  function dur(ms, withSec = false) {
    const t = Math.max(0, Math.floor(ms / 1000));
    const h = Math.floor(t / 3600), m = Math.floor((t % 3600) / 60), s = t % 60;
    if (withSec) return h ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
    if (h) return `${h} giờ${m ? ` ${m} phút` : ''}`;
    return `${m} phút`;
  }
  const dDate = t => { const d = new Date(t); return `${pad(d.getDate())}/${pad(d.getMonth() + 1)}/${d.getFullYear()}`; };
  const dTime = t => { const d = new Date(t); return `${pad(d.getHours())}:${pad(d.getMinutes())}`; };
  const dayKey = t => { const d = new Date(t); return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`; };
  const toLocalInput = t => { const d = new Date(t); return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`; };
  const hearts = n => `<span class="hearts" aria-label="${n}/5 tim">${[1, 2, 3, 4, 5].map(i => icon('heart', i <= n ? 'on' : 'off')).join('')}</span>`;
  const mode = arr => {
    const m = new Map();
    let best = null, bn = 0;
    for (const x of arr) { const n = (m.get(x) || 0) + 1; m.set(x, n); if (n > bn) { bn = n; best = x; } }
    return best;
  };
  const groupBy = (arr, fn) => {
    const m = new Map();
    for (const x of arr) { const k = fn(x); if (!m.has(k)) m.set(k, []); m.get(k).push(x); }
    return m;
  };
  const centroid = list => [list.reduce((a, p) => a + p.lat, 0) / list.length, list.reduce((a, p) => a + p.lng, 0) / list.length];

  let toastTimer;
  function toast(msg) {
    const el = $('#toast');
    el.textContent = msg;
    el.classList.add('show');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => el.classList.remove('show'), 2600);
  }

  function ask(title, text, yes = 'Xoá', danger = true) {
    return new Promise(resolve => {
      const m = $('#modal');
      $('#modalTitle').textContent = title;
      $('#modalText').textContent = text;
      const y = $('#modalYes'), n = $('#modalNo');
      y.textContent = yes;
      y.className = `btn ${danger ? 'danger' : 'primary'}`;
      m.hidden = false;
      const done = v => { m.hidden = true; y.onclick = n.onclick = m.onclick = null; resolve(v); };
      y.onclick = () => done(true);
      n.onclick = () => done(false);
      m.onclick = e => { if (e.target === m) done(false); };
    });
  }

  /* ================= Khu vực: quốc gia › tỉnh/thành › địa điểm ================= */
  const cKey = c => c.cc || (c.country ? 'x:' + Geo.norm(c.country) : '??');
  const rKey = c => `${cKey(c)}|${c.province || ''}`;
  const rLabel = c => c.province || c.country || 'Chưa rõ khu vực';
  const cLabel = c => c.country || 'Chưa rõ khu vực';
  const whereText = c => [c.province, c.country].filter(Boolean).join(', ');
  const flagOf = c => Geo.flag(c.cc);

  /** Gộp các lần check-in cùng tên, cùng tỉnh và cách nhau dưới 300 m thành một địa điểm. */
  function places() {
    if (S._places) return S._places;
    const byName = new Map();
    const out = [];
    for (const c of S.checkins) { // đã sắp mới nhất trước
      const k = c.name.trim().toLowerCase() + '|' + rKey(c);
      const cands = byName.get(k) || [];
      let p = cands.find(x => hav([x.lat, x.lng], [c.lat, c.lng]) < 300);
      if (!p) {
        p = { key: k + '#' + cands.length, latest: c, visits: [], lat: c.lat, lng: c.lng, name: c.name, cat: c.cat, cc: c.cc, country: c.country, province: c.province, demo: c.demo };
        cands.push(p);
        byName.set(k, cands);
        out.push(p);
      }
      p.visits.push(c);
    }
    out.forEach(p => {
      p.rating = (p.visits.find(v => v.rating) || {}).rating || 0;
      p.photoId = (p.visits.find(v => v.photoIds.length) || { photoIds: [] }).photoIds[0] || null;
      p.price = (p.visits.find(v => v.price != null) || {}).price ?? null;
      p.tags = [...new Set(p.visits.flatMap(v => v.tags))];
      p.feedback = (p.visits.find(v => v.feedback) || {}).feedback || '';
      p.photos = p.visits.reduce((a, v) => a + v.photoIds.length, 0);
    });
    return (S._places = out);
  }
  const placeOf = id => places().find(p => p.visits.some(v => v.id === id));

  const geoCache = new Map();
  async function reverse(lat, lng) {
    const key = lat.toFixed(4) + ',' + lng.toFixed(4);
    if (geoCache.has(key)) return geoCache.get(key);
    try {
      const res = await fetch(`https://nominatim.openstreetmap.org/reverse?format=jsonv2&lat=${lat}&lon=${lng}&zoom=18&addressdetails=1&accept-language=vi`);
      if (!res.ok) return null;
      const r = await res.json();
      geoCache.set(key, r);
      return r;
    } catch { return null; }
  }

  /** Điền tỉnh/quốc gia cho các check-in cũ chưa có: đoán ngay, rồi hỏi lại dịch vụ bản đồ khi có mạng. */
  async function backfillRegions() {
    let changed = false;
    for (const c of S.checkins) {
      if (c.regionSrc) continue;
      const g = Geo.guess(c.lat, c.lng);
      if (g) Object.assign(c, { province: g.province, country: g.country, cc: g.cc, regionSrc: 'guess' });
      else c.regionSrc = 'none';
      await DB.put('checkins', c);
      changed = true;
    }
    if (changed) bump();
    const todo = S.checkins.filter(c => c.regionSrc === 'guess' || c.regionSrc === 'none').slice(0, 12);
    for (const c of todo) {
      const r = await reverse(c.lat, c.lng);
      if (!r) break; // không có mạng: để lần sau
      const reg = Geo.fromAddress(r.address, c.lat, c.lng);
      if (reg) {
        Object.assign(c, { province: reg.province, country: reg.country, cc: reg.cc, regionSrc: 'geo' });
        await DB.put('checkins', c);
        changed = true;
      }
      await sleep(1200);
    }
    if (todo.length && changed) bump();
  }

  /* ================= Ảnh ================= */
  const urlCache = new Map();
  async function photoUrl(id, size = 'thumb') {
    const key = id + ':' + size;
    if (urlCache.has(key)) return urlCache.get(key);
    const p = await DB.get('photos', id);
    if (!p) return '';
    const u = URL.createObjectURL(size === 'thumb' ? (p.thumb || p.blob) : (p.blob || p.thumb));
    urlCache.set(key, u);
    return u;
  }
  function dropUrls(id) {
    ['thumb', 'full'].forEach(s => {
      const k = id + ':' + s;
      if (urlCache.has(k)) { URL.revokeObjectURL(urlCache.get(k)); urlCache.delete(k); }
    });
  }
  function hydratePhotos(root) {
    $$('img[data-photo]', root).forEach(async img => {
      if (img.getAttribute('src')) return;
      const u = await photoUrl(img.dataset.photo, img.dataset.size || 'thumb');
      if (u) img.src = u;
    });
  }
  function loadImage(file) {
    return new Promise((res, rej) => {
      const u = URL.createObjectURL(file);
      const img = new Image();
      img.onload = () => { res(img); setTimeout(() => URL.revokeObjectURL(u), 1000); };
      img.onerror = () => { URL.revokeObjectURL(u); rej(new Error('decode')); };
      img.src = u;
    });
  }
  function resize(img, max, q) {
    const r = Math.min(1, max / Math.max(img.naturalWidth, img.naturalHeight));
    const w = Math.round(img.naturalWidth * r), h = Math.round(img.naturalHeight * r);
    const cv = document.createElement('canvas');
    cv.width = w; cv.height = h;
    cv.getContext('2d').drawImage(img, 0, 0, w, h);
    return new Promise(res => cv.toBlob(res, 'image/jpeg', q));
  }
  async function processImage(file) {
    try {
      const img = await loadImage(file);
      const [blob, thumb] = await Promise.all([resize(img, 1600, 0.82), resize(img, 420, 0.78)]);
      if (!blob || !thumb) throw new Error('encode');
      return { blob, thumb };
    } catch {
      return { blob: file, thumb: file }; // định dạng trình duyệt không đọc được: giữ nguyên tệp
    }
  }

  /* ================= Bản đồ ================= */
  let map, tileLayer, meMarker, accCircle, activeLine, tempMarker, vnOutline;
  const L_pins = L.layerGroup();
  const L_routes = L.layerGroup();
  const L_focus = L.layerGroup();
  const L_isles = L.layerGroup();

  const level = () => { const z = map.getZoom(); return z < Z_COUNTRY ? 'world' : z < Z_LOCAL ? 'country' : 'local'; };
  // Phần bản đồ bị các tấm kính che: trên là ô tìm kiếm, dưới là thẻ ngữ cảnh + thanh tab.
  const padTL = () => L.point(16, 96);
  const padBR = () => L.point(16, Math.min(300, ($('#scopeCard').offsetHeight || 130) + 112));

  function initMap() {
    map = L.map('map', { zoomControl: false, attributionControl: false, zoomSnap: 0.25, zoomDelta: 0.5, minZoom: 2, maxZoom: 19, worldCopyJump: true });
    map.fitBounds(VN_VIEW, { paddingTopLeft: padTL(), paddingBottomRight: L.point(16, 250), animate: false });
    setTile(S.tile);

    vnOutline = L.polygon(VN_SHAPE, { color: '#EC7FA2', weight: 2, opacity: .85, fillColor: '#F6AFC6', fillOpacity: .13, interactive: false, smoothFactor: 0.5 });
    ISLES.forEach(i => L.marker([i.lat, i.lng], { interactive: false, icon: L.divIcon({ className: 'mk', html: `<span class="isle">${i.name}</span>`, iconSize: [0, 0] }) }).addTo(L_isles));
    L_focus.addTo(map);
    L_pins.addTo(map);
    activeLine = L.polyline([], { color: '#EC7FA2', weight: 6, opacity: .95, lineCap: 'round', lineJoin: 'round' }).addTo(map);

    map.on('dragstart', () => { S.follow = false; $('#btnLocate').classList.remove('is-on'); });
    map.on('contextmenu', e => { if (!S.picking) openCheckinForm({ lat: e.latlng.lat, lng: e.latlng.lng, fromMap: true }); });
    map.on('click', () => { $('#searchResults').hidden = true; $('#layerMenu').hidden = true; });
    map.on('zoomend', () => { syncLayers(); drawPins(); renderScope(); });
    let mt;
    map.on('moveend', () => { clearTimeout(mt); mt = setTimeout(renderScope, 120); });
    syncLayers();
  }

  function syncLayers() {
    const z = map.getZoom();
    const toggle = (layer, on) => { if (on && !map.hasLayer(layer)) layer.addTo(map); else if (!on && map.hasLayer(layer)) map.removeLayer(layer); };
    toggle(vnOutline, z < Z_LOCAL && S.tile !== 'sat');
    toggle(L_isles, z >= Z_COUNTRY && z < Z_LOCAL + 1);
    toggle(L_routes, z >= Z_ROUTES);
    if (meMarker) meMarker.setZIndexOffset(z < Z_LOCAL ? -1000 : 1000);
  }

  function setTile(key) {
    if (!TILES[key]) key = 'voyager';
    if (tileLayer) map.removeLayer(tileLayer);
    const t = TILES[key];
    tileLayer = L.tileLayer(t.url, t.opt).addTo(map);
    tileLayer.bringToBack();
    $('#map').classList.toggle('is-sat', key === 'sat');
    $('#attrib').innerHTML = t.by;
    S.tile = key;
    if (vnOutline) syncLayers();
  }

  function addMarker(lat, lng, html, size, anchor, onClick, z = 0) {
    const m = L.marker([lat, lng], { icon: L.divIcon({ className: 'mk', html, iconSize: [size, size], iconAnchor: anchor || [size / 2, size / 2] }), zIndexOffset: z, keyboard: false });
    if (onClick) m.on('click', onClick);
    return m.addTo(L_pins);
  }
  const bubSize = n => Math.round(40 + Math.min(22, Math.sqrt(n) * 6));

  /** Đẩy nhẹ các bong bóng (kèm nhãn tên) ra xa nhau để không che nhau khi hai vùng nằm sát. */
  function spread(items, z) {
    items.forEach(it => {
      it.pt = map.project(it.at, z);
      it.w = Math.max(it.s, Math.min(170, it.label.length * 7.2 + 20)) + 6;
      it.h = it.s + 30;
    });
    for (let n = 0; n < 10; n++) {
      let moved = false;
      for (let i = 0; i < items.length; i++) for (let k = i + 1; k < items.length; k++) {
        const a = items[i], b = items[k];
        const dx = b.pt.x - a.pt.x, dy = b.pt.y - a.pt.y;
        const ox = (a.w + b.w) / 2 - Math.abs(dx), oy = (a.h + b.h) / 2 - Math.abs(dy);
        if (ox <= 0 || oy <= 0) continue;
        moved = true;
        if (ox < oy) { const m = (ox / 2) * (dx < 0 ? -1 : 1); a.pt.x -= m; b.pt.x += m; }
        else { const m = (oy / 2) * (dy < 0 ? -1 : 1); a.pt.y -= m; b.pt.y += m; }
      }
      if (!moved) break;
    }
    items.forEach(it => { const ll = map.unproject(it.pt, z); it.at = [ll.lat, ll.lng]; });
    return items;
  }

  /** Vẽ ghim theo cấp zoom: quốc gia → tỉnh/thành → cụm gần nhau → từng địa điểm. */
  function drawPins() {
    L_pins.clearLayers();
    const P = places();
    if (!P.length) return;
    const lv = level(), z = map.getZoom();

    if (lv === 'world') {
      const items = [...groupBy(P, cKey)].map(([ck, list]) => ({ ck, list, s: 52, label: cLabel(list[0]), at: ck === 'VN' ? [16.2, 106.6] : centroid(list) }));
      spread(items, z).forEach(it => addMarker(it.at[0], it.at[1], `<div class="bub country">${flagOf(it.list[0])}<em>${it.list.length}</em></div><span class="mk-name">${esc(it.label)}</span>`, 52, null, () => flyCountry(it.ck), it.list.length));
      return;
    }
    if (lv === 'country') {
      const items = [...groupBy(P, rKey)].map(([rk, list]) => ({ rk, list, s: bubSize(list.length), label: rLabel(list[0]), at: centroid(list) }));
      spread(items, z).forEach(it => addMarker(it.at[0], it.at[1], `<div class="bub">${it.list.length}</div><span class="mk-name">${esc(it.label)}</span>`, it.s, null, () => flyRegion(it.rk), it.list.length));
      return;
    }

    // Cấp địa phương: gom các điểm sát nhau trên màn hình, zoom đủ gần thì tách hết.
    const clusters = [];
    const R = z >= 16.5 ? 0 : 46;
    for (const p of P) {
      const pt = map.project([p.lat, p.lng], z);
      let c = R ? clusters.find(k => Math.abs(k.pt.x - pt.x) < R && Math.abs(k.pt.y - pt.y) < R) : null;
      if (!c) clusters.push(c = { pt, list: [] });
      c.list.push(p);
    }
    const detail = z >= Z_DETAIL;
    for (const c of clusters) {
      if (c.list.length > 1) {
        const [lat, lng] = centroid(c.list);
        addMarker(lat, lng, `<div class="bub mini">${c.list.length}</div>`, 36, null, () => {
          const b = L.latLngBounds(c.list.map(p => [p.lat, p.lng]));
          map.flyToBounds(b.pad(0.4), { paddingTopLeft: padTL(), paddingBottomRight: padBR(), maxZoom: 18, duration: .7 });
        }, 500);
        continue;
      }
      const p = c.list[0];
      const e = catOf(p).e;
      const fav = p.rating >= 5 ? 'fav' : '';
      if (detail) {
        addMarker(p.lat, p.lng, `<div class="pin photo ${fav}">${p.photoId ? `<img data-photo="${p.photoId}" alt="">` : `<span>${e}</span>`}</div><span class="mk-name">${esc(p.name)}</span>`, 46, null, () => openDetail(p.latest.id));
      } else {
        addMarker(p.lat, p.lng, `<div class="pin ${fav}"><span>${e}</span></div>`, 36, [18, 45], () => openDetail(p.latest.id));
      }
    }
    if (detail) hydratePhotos(map.getPanes().markerPane);
  }

  function segLines(points) {
    const lines = [];
    let cur = null, seg = null;
    for (const p of points) {
      if (p[3] !== seg) { cur = []; lines.push(cur); seg = p[3]; }
      cur.push([p[0], p[1]]);
    }
    return lines;
  }
  function drawRoutes() {
    L_routes.clearLayers();
    S.journeys.filter(j => j.status === 'done' && j.points.length > 1).forEach(j => {
      L.polyline(segLines(j.points), { color: '#C24E78', weight: 4, opacity: .5, dashArray: '1 8', lineCap: 'round' })
        .bindTooltip(j.name, { sticky: true, className: 'route-tip' })
        .on('click', () => openJourney(j.id))
        .addTo(L_routes);
    });
  }
  const drawActive = () => activeLine.setLatLngs(S.active ? segLines(S.active.points) : []);

  function updateMe(lat, lng, acc) {
    S.me = { lat, lng, acc, t: Date.now() };
    if (!meMarker) {
      meMarker = L.marker([lat, lng], { icon: L.divIcon({ className: 'mk', html: '<div class="me-dot"></div>', iconSize: [18, 18], iconAnchor: [9, 9] }), interactive: false, zIndexOffset: 1000 }).addTo(map);
      accCircle = L.circle([lat, lng], { radius: acc, color: '#EC7FA2', weight: 1, opacity: .4, fillColor: '#FBDCE6', fillOpacity: .25, interactive: false }).addTo(map);
      meMarker.setZIndexOffset(map.getZoom() < Z_LOCAL ? -1000 : 1000);
    } else {
      meMarker.setLatLng([lat, lng]);
      accCircle.setLatLng([lat, lng]).setRadius(acc);
    }
  }

  /* --- di chuyển giữa các cấp --- */
  const flyOpt = () => ({ paddingTopLeft: padTL(), paddingBottomRight: padBR(), duration: .9 });
  // Dời tâm xuống một chút để vùng cần xem nằm giữa phần bản đồ không bị che.
  function offsetCenter(latlng, z) {
    const dy = (padBR().y - padTL().y) / 2;
    return map.unproject(map.project(latlng, z).add([0, dy]), z);
  }
  function showS(animate = true) {
    S.follow = false;
    if (animate) map.flyToBounds(VN_VIEW, flyOpt());
    else map.fitBounds(VN_VIEW, { paddingTopLeft: padTL(), paddingBottomRight: padBR(), animate: false });
  }
  function showWorld() {
    S.follow = false;
    const P = places();
    if (!P.length) { map.flyTo([16, 106], 3.5, { duration: .9 }); return; }
    const b = L.latLngBounds(P.map(p => [p.lat, p.lng])).pad(0.5);
    const z = Math.max(2.5, Math.min(Z_COUNTRY - 0.5, map.getBoundsZoom(b, false, L.point(40, 360))));
    map.flyTo(offsetCenter(b.getCenter(), z), z, { duration: .9 });
  }
  function flyCountry(ck) {
    S.follow = false;
    if (ck === 'VN') return showS();
    const list = places().filter(p => cKey(p) === ck);
    if (!list.length) return;
    const b = L.latLngBounds(list.map(p => [p.lat, p.lng])).pad(0.8);
    const z = Math.max(Z_COUNTRY + 0.5, Math.min(Z_LOCAL - 1, map.getBoundsZoom(b, false, L.point(40, 360))));
    map.flyTo(offsetCenter(b.getCenter(), z), z, { duration: .9 });
  }
  function flyRegion(rk) {
    S.follow = false;
    const list = places().filter(p => rKey(p) === rk);
    if (!list.length) return;
    const b = L.latLngBounds(list.map(p => [p.lat, p.lng])).pad(0.35);
    const z = Math.max(Z_LOCAL + 0.5, Math.min(15, map.getBoundsZoom(b, false, L.point(60, 380))));
    map.flyTo(offsetCenter(b.getCenter(), z), z, { duration: .9 });
  }
  function flyPlace(p, open = true) {
    S.follow = false;
    const z = Math.max(map.getZoom(), 16);
    map.flyTo(offsetCenter([p.lat, p.lng], z), z, { duration: .8 });
    if (open) openDetail(p.latest.id);
  }

  /* --- thẻ ngữ cảnh ở đáy bản đồ --- */
  const crumbs = parts => `<nav class="crumbs">${parts.map((p, i) => `${i ? icon('chevron') : ''}<button type="button" ${p.attr || 'disabled'}>${p.label}</button>`).join('')}</nav>`;

  function renderScope() {
    const el = $('#scopeCard');
    el.hidden = S.picking || S.view !== 'map';
    if (el.hidden || !map) return;
    const P = places();
    let key, html;

    if (!P.length) {
      key = 'empty';
      html = `
        <div class="scope-head"><h2>Bản đồ chữ S của bạn</h2><span class="num"><b>0</b>/${Geo.UNITS.length} tỉnh thành</span></div>
        <p>Chưa có dấu chân nào. Bấm nút <b>+</b> để check-in nơi đầu tiên, hoặc xem thử với dữ liệu mẫu.</p>
        <div><button type="button" class="btn soft sm" data-act="demo">${icon('sparkle')}Xem thử dữ liệu mẫu</button></div>`;
    } else {
      const lv = level();
      const b = map.getBounds();
      const inView = P.filter(p => b.contains([p.lat, p.lng]));
      const ctr = map.getCenter();

      if (lv === 'world') {
        const groups = [...groupBy(P, cKey)].sort((a, b2) => b2[1].length - a[1].length);
        key = 'world';
        html = `
          ${crumbs([{ label: 'Thế giới' }])}
          <div class="scope-head"><h2>Thế giới của bạn</h2><span class="num"><b>${groups.length}</b> quốc gia · ${P.length} địa điểm</span></div>
          <div class="hscroll">${groups.map(([ck, list]) => `<button type="button" class="reg-chip" data-cc="${esc(ck)}"><i class="flag">${flagOf(list[0])}</i>${esc(cLabel(list[0]))}<em>${list.length}</em></button>`).join('')}</div>`;
      } else if (lv === 'country') {
        let ck = mode(inView.map(cKey));
        if (!ck && Geo.inVietnam(ctr.lat, ctr.lng)) ck = 'VN';
        const list = ck ? P.filter(p => cKey(p) === ck) : [];
        key = 'country:' + ck;
        if (ck === 'VN') {
          const groups = [...groupBy(list, rKey)].sort((a, b2) => b2[1].length - a[1].length);
          html = `
            ${crumbs([{ label: 'Thế giới', attr: 'data-act="world"' }, { label: 'Việt Nam' }])}
            <div class="scope-head"><h2>Việt Nam</h2><span class="num"><b>${groups.length}</b>/${Geo.UNITS.length} tỉnh thành</span></div>
            <div class="meter"><span style="width:${(groups.length / Geo.UNITS.length) * 100}%"></span></div>
            ${groups.length
              ? `<div class="hscroll">${groups.map(([rk, l]) => `<button type="button" class="reg-chip" data-rk="${esc(rk)}"><i>${l.length}</i>${esc(rLabel(l[0]))}</button>`).join('')}</div>`
              : '<p>Bạn chưa ghim nơi nào ở Việt Nam. Bấm <b>+</b> để bắt đầu.</p>'}`;
        } else if (ck) {
          const groups = [...groupBy(list, rKey)].sort((a, b2) => b2[1].length - a[1].length);
          html = `
            ${crumbs([{ label: 'Thế giới', attr: 'data-act="world"' }, { label: esc(cLabel(list[0])) }])}
            <div class="scope-head"><h2>${flagOf(list[0])} ${esc(cLabel(list[0]))}</h2><span class="num"><b>${list.length}</b> địa điểm</span></div>
            <div class="hscroll">${groups.map(([rk, l]) => `<button type="button" class="reg-chip" data-rk="${esc(rk)}"><i>${l.length}</i>${esc(rLabel(l[0]))}</button>`).join('')}</div>`;
        } else {
          html = `
            ${crumbs([{ label: 'Thế giới', attr: 'data-act="world"' }])}
            <div class="scope-head"><h2>Vùng này chưa có dấu chân</h2></div>
            <div class="hscroll"><button type="button" class="reg-chip" data-act="home"><i class="flag">🇻🇳</i>Về bản đồ chữ S</button></div>`;
        }
      } else {
        const rk = mode(inView.map(rKey));
        if (rk) {
          const list = P.filter(p => rKey(p) === rk);
          const first = list[0];
          const rated = list.filter(p => p.rating);
          const avg = rated.length ? rated.reduce((a, p) => a + p.rating, 0) / rated.length : 0;
          const ordered = list.filter(p => inView.includes(p)).concat(list.filter(p => !inView.includes(p)));
          key = 'region:' + rk;
          html = `
            ${crumbs([{ label: 'Thế giới', attr: 'data-act="world"' }, { label: esc(cLabel(first)), attr: `data-cc="${esc(cKey(first))}"` }, { label: esc(rLabel(first)) }])}
            <div class="scope-head"><h2>${esc(rLabel(first))}</h2><span class="num"><b>${list.length}</b> địa điểm${avg ? ` · ♥ ${nf1.format(avg)}` : ''}</span></div>
            <div class="hscroll">${ordered.map(p => `
              <button type="button" class="place-chip" data-place="${p.latest.id}">
                <span class="th">${p.photoId ? `<img data-photo="${p.photoId}" alt="">` : catOf(p).e}</span>
                <span><b>${esc(p.name)}</b><small>${catOf(p).e} ${p.rating ? '♥'.repeat(p.rating) : catOf(p).label}</small></span>
              </button>`).join('')}</div>`;
        } else {
          let near = null, nd = Infinity;
          for (const p of P) { const d = hav([ctr.lat, ctr.lng], [p.lat, p.lng]); if (d < nd) { nd = d; near = p; } }
          const inVN = Geo.inVietnam(ctr.lat, ctr.lng);
          key = 'none:' + near.key;
          html = `
            ${crumbs([{ label: 'Thế giới', attr: 'data-act="world"' }].concat(inVN ? [{ label: 'Việt Nam', attr: 'data-cc="VN"' }] : []))}
            <div class="scope-head"><h2>Chưa có dấu chân quanh đây</h2></div>
            <div class="hscroll">
              <button type="button" class="reg-chip" data-act="pickpin"><i>${icon('plus')}</i>Ghim nơi này</button>
              <button type="button" class="reg-chip" data-place="${near.latest.id}"><i class="flag">${catOf(near).e}</i>Gần nhất: ${esc(near.name)}<em>${km(nd)}</em></button>
            </div>`;
        }
      }
    }
    key += '|' + S.rev;
    if (el.dataset.key === key) return;
    el.dataset.key = key;
    el.innerHTML = html;
    hydratePhotos(el);
  }

  /* ================= Định vị ================= */
  const geoOk = 'geolocation' in navigator;
  const GEO_OPT = { enableHighAccuracy: true, maximumAge: 3000, timeout: 20000 };

  function geoError(err) {
    const msgs = {
      1: 'Chưa có quyền vị trí. Hãy bật quyền vị trí cho trang này trong cài đặt trình duyệt.',
      2: 'Không xác định được vị trí. Hãy ra chỗ thoáng hoặc bật GPS.',
      3: 'Lấy vị trí quá lâu, đang thử lại…'
    };
    S.gpsMsg = msgs[err.code] || 'Không lấy được vị trí.';
    S.gpsWarn = true;
    renderRec();
    if (err.code === 1) toast('Cần quyền truy cập vị trí');
    else if (!S.active) toast(S.gpsMsg);
  }

  function locateOnce(fly = true) {
    if (!geoOk) { toast('Trình duyệt không hỗ trợ định vị'); return Promise.resolve(null); }
    return new Promise(resolve => {
      navigator.geolocation.getCurrentPosition(pos => {
        const { latitude: lat, longitude: lng, accuracy } = pos.coords;
        updateMe(lat, lng, accuracy);
        if (fly) { const z = Math.max(map.getZoom(), 16); map.flyTo(offsetCenter([lat, lng], z), z, { duration: .8 }); }
        resolve(S.me);
      }, err => { geoError(err); resolve(null); }, GEO_OPT);
    });
  }
  function startWatch() {
    if (!geoOk || S.watchId !== null) return;
    S.watchId = navigator.geolocation.watchPosition(onPosition, geoError, GEO_OPT);
  }
  function stopWatch() {
    if (S.watchId !== null) navigator.geolocation.clearWatch(S.watchId);
    S.watchId = null;
  }

  function onPosition(pos) {
    const { latitude: lat, longitude: lng, accuracy } = pos.coords;
    updateMe(lat, lng, accuracy);
    const j = S.active;
    if (S.follow && S.view === 'map' && !S.picking) {
      if (map.getZoom() < 15) map.setView(offsetCenter([lat, lng], 16), 16);
      else map.panTo(offsetCenter([lat, lng], map.getZoom()), { animate: true });
    }
    if (!j || j.status !== 'active') return;

    if (accuracy > 50) {
      S.gpsMsg = `GPS yếu (±${Math.round(accuracy)} m), đang chờ chính xác hơn`;
      S.gpsWarn = true;
      renderRec();
      return;
    }
    S.gpsMsg = '';
    S.gpsWarn = false;

    const now = pos.timestamp || Date.now();
    const last = j.points[j.points.length - 1];
    if (!last || last[3] !== j.seg) {
      j.points.push([+lat.toFixed(6), +lng.toFixed(6), now, j.seg]);
    } else {
      const d = hav(last, [lat, lng]);
      const dt = Math.max(1, (now - last[2]) / 1000);
      if (d < 8 || d / dt > 70) { renderRec(); return; } // đứng yên hoặc nhảy điểm bất thường
      j.points.push([+lat.toFixed(6), +lng.toFixed(6), now, j.seg]);
      j.distance += d;
    }
    drawActive();
    renderRec();
    if (Date.now() - S.lastSave > 8000) saveActive();
  }

  async function saveActive() {
    if (!S.active) return;
    S.lastSave = Date.now();
    await DB.put('journeys', S.active);
  }
  async function wake(on) {
    try {
      if (on && 'wakeLock' in navigator && !S.wakeLock) {
        S.wakeLock = await navigator.wakeLock.request('screen');
        S.wakeLock.addEventListener('release', () => { S.wakeLock = null; });
      } else if (!on && S.wakeLock) {
        await S.wakeLock.release();
        S.wakeLock = null;
      }
    } catch { /* không hỗ trợ: bỏ qua */ }
  }

  /* ================= Hành trình ================= */
  const elapsed = j => j.activeMs + (j.status === 'active' && j.resumedAt ? Date.now() - j.resumedAt : 0);
  const jCheckins = id => S.checkins.filter(c => c.journeyId === id);
  function jRegion(j) {
    const cs = jCheckins(j.id);
    if (cs.length) { const rk = mode(cs.map(rKey)); return cs.find(c => rKey(c) === rk); }
    if (j.province || j.country) return j;
    return j.points.length ? Geo.guess(j.points[0][0], j.points[0][1]) : null;
  }

  async function startJourney() {
    if (!geoOk) { toast('Trình duyệt không hỗ trợ định vị'); return; }
    const now = Date.now();
    const d = new Date(now);
    const part = d.getHours() < 11 ? 'Buổi sáng' : d.getHours() < 14 ? 'Buổi trưa' : d.getHours() < 18 ? 'Buổi chiều' : 'Buổi tối';
    S.active = {
      id: uid(), name: `${part} ${pad(d.getDate())}/${pad(d.getMonth() + 1)}`, note: '',
      startedAt: now, endedAt: null, points: [], distance: 0, activeMs: 0, resumedAt: now, status: 'active', seg: 0
    };
    S.journeys.unshift(S.active);
    S.follow = true;
    $('#btnLocate').classList.add('is-on');
    S.gpsMsg = 'Đang bắt tín hiệu GPS…';
    S.gpsWarn = false;
    await saveActive();
    go('map');
    startWatch();
    wake(true);
    renderRec();
    toast('Bắt đầu ghi hành trình. Giữ màn hình mở nhé.');
  }
  async function pauseJourney() {
    const j = S.active;
    j.activeMs = elapsed(j);
    j.resumedAt = null;
    j.status = 'paused';
    stopWatch();
    wake(false);
    await saveActive();
    renderRec();
  }
  async function resumeJourney() {
    const j = S.active;
    j.status = 'active';
    j.resumedAt = Date.now();
    j.seg = (j.seg || 0) + 1; // đoạn mới, không nối thẳng qua khoảng tạm dừng
    S.follow = true;
    await saveActive();
    startWatch();
    wake(true);
    renderRec();
  }
  async function endJourney() {
    const j = S.active;
    if (!j) return;
    j.activeMs = elapsed(j);
    j.resumedAt = null;
    j.status = 'done';
    j.endedAt = Date.now();
    stopWatch();
    wake(false);
    await DB.put('journeys', j);
    S.active = null;
    drawActive();
    drawRoutes();
    bump();
    openJourneySummary(j);
  }
  const askEnd = () => ask('Kết thúc hành trình?', 'Lộ trình sẽ được lưu vào Nhật ký. Bạn có thể đặt tên và ghi cảm nhận.', 'Kết thúc', false).then(ok => ok && endJourney());

  function openJourneySummary(j) {
    const cs = jCheckins(j.id);
    const empty = j.distance < 30 && !cs.length;
    openSheet(`
      <div class="sheet-head"><h2>Hoàn thành hành trình</h2></div>
      <div class="jstats">
        <div><b>${km(j.distance)}</b><span>Quãng đường</span></div>
        <div><b>${dur(j.activeMs)}</b><span>Thời gian di chuyển</span></div>
        <div><b>${cs.length}</b><span>Điểm check-in</span></div>
        <div><b>${nf.format(Math.round(j.distance / STRIDE_M))}</b><span>Bước chân (ước tính)</span></div>
      </div>
      <div class="field"><label for="jName">Đặt tên cho hành trình</label><input class="input" id="jName" value="${esc(j.name)}" maxlength="80"></div>
      <div class="field"><label for="jNote">Cảm nhận về chuyến đi <span class="hint">· không bắt buộc</span></label><textarea class="input script" id="jNote" placeholder="Hôm nay mình đã…">${esc(j.note)}</textarea></div>
      ${empty ? '<p class="foot-note">Hành trình này gần như chưa di chuyển. Bạn có thể bỏ nó đi.</p>' : ''}
    `, `
      ${empty ? '<button type="button" class="btn ghost" data-act="discard">Bỏ hành trình</button>' : ''}
      <button type="button" class="btn primary" data-act="save">Lưu vào Nhật ký</button>
    `, 'summary');
    $('#sheetFoot').onclick = async e => {
      const act = e.target.closest('[data-act]')?.dataset.act;
      if (act === 'save') {
        j.name = $('#jName').value.trim() || j.name;
        j.note = $('#jNote').value.trim();
        await DB.put('journeys', j);
        closeSheet();
        toast('Đã lưu hành trình vào Nhật ký');
        bump();
      } else if (act === 'discard') {
        await deleteJourney(j.id, true);
        closeSheet();
      }
    };
  }

  async function deleteJourney(id, silent = false) {
    for (const c of jCheckins(id)) { c.journeyId = null; await DB.put('checkins', c); }
    await DB.del('journeys', id);
    S.journeys = S.journeys.filter(j => j.id !== id);
    if (S.focusId === id) unfocus();
    drawRoutes();
    bump();
    if (!silent) toast('Đã xoá hành trình');
  }

  /* --- thanh ghi hành trình trên bản đồ --- */
  function renderRec() {
    const el = $('#recBar');
    const j = S.active;
    $('.tab[data-go="map"]').classList.toggle('rec', !!j && j.status === 'active');
    el.hidden = !j || S.picking || S.view !== 'map';
    if (!j) { el.dataset.key = ''; return; }
    const ms = elapsed(j);
    const paused = j.status === 'paused';
    const vals = [kmNum(j.distance), dur(ms, true), jCheckins(j.id).length];
    const sub = paused ? `${j.name} · đang tạm dừng` : (S.gpsMsg || `${j.name} · đang ghi`);
    const key = [j.id, j.status, sub, S.gpsWarn].join('|');
    if (el.dataset.key === key) { // chỉ cập nhật số, giữ nguyên các nút
      $$('[data-v]', el).forEach((b, i) => { b.textContent = vals[i]; });
      return;
    }
    el.dataset.key = key;
    el.innerHTML = `
      <span class="rec-dot ${paused ? 'paused' : ''}"></span>
      <span class="t">
        <span class="nums"><b data-v>${vals[0]}</b><span>km</span><b data-v>${vals[1]}</b><span></span><b data-v>${vals[2]}</b><span>điểm</span></span>
        <span class="nm ${S.gpsWarn && !paused ? 'warn' : ''}">${esc(sub)}</span>
      </span>
      ${paused
        ? `<button type="button" class="sq soft" data-act="resume" aria-label="Tiếp tục">${icon('play')}</button>`
        : `<button type="button" class="sq soft" data-act="pause" aria-label="Tạm dừng">${icon('pause')}</button>`}
      <button type="button" class="sq stop" data-act="end" aria-label="Kết thúc hành trình">${icon('stop')}</button>`;
  }

  /* ================= Menu tạo mới (nút +) ================= */
  function openFab() {
    const j = S.active;
    const item = (act, ic, cls, title, sub) => `<button type="button" data-act="${act}"><span class="ic ${cls}">${icon(ic)}</span><span class="t"><b>${title}</b><small>${sub}</small></span></button>`;
    $('#fabMenu').innerHTML =
      item('checkin', 'pin', 'main', 'Check-in tại đây', 'Lưu nơi bạn đang đứng, kèm ảnh và cảm nhận') +
      item('pickpin', 'map', '', 'Ghim một điểm trên bản đồ', 'Cho nơi đã đi qua mà chưa kịp check-in') +
      (!j ? item('start', 'route', '', 'Ghi hành trình', 'Vẽ lại con đường bạn đi bằng GPS')
        : (j.status === 'paused'
          ? item('resume', 'play', '', 'Tiếp tục hành trình', esc(j.name))
          : item('pause', 'pause', '', 'Tạm dừng hành trình', esc(j.name)))
          + item('end', 'stop', 'stop', 'Kết thúc hành trình', `${km(j.distance)} · ${dur(elapsed(j))}`));
    $('#fabMenu').hidden = false;
    $('#fabScrim').hidden = false;
    $('#btnAdd').setAttribute('aria-expanded', 'true');
  }
  function closeFab() {
    $('#fabMenu').hidden = true;
    $('#fabScrim').hidden = true;
    $('#btnAdd').setAttribute('aria-expanded', 'false');
  }
  function doAct(act) {
    if (act === 'checkin') openCheckinForm();
    else if (act === 'pickpin') pickNew();
    else if (act === 'start') startJourney();
    else if (act === 'pause') pauseJourney();
    else if (act === 'resume') resumeJourney();
    else if (act === 'end') askEnd();
    else if (act === 'demo') addDemo();
    else if (act === 'cleardemo') clearDemo();
    else if (act === 'export') exportData();
    else if (act === 'import') $('#fileImport').click();
    else if (act === 'wipe') wipeAll();
    else if (act === 'world') showWorld();
    else if (act === 'home') { go('map'); showS(); }
    else return false;
    return true;
  }

  /* ================= Bottom sheet ================= */
  function openSheet(html, foot = '', kind = null) {
    closeFab();
    S.sheetKind = kind;
    const inner = $('#sheetInner');
    inner.innerHTML = html;
    inner.onclick = null;
    const f = $('#sheetFoot');
    f.innerHTML = foot;
    f.hidden = !foot;
    f.onclick = null;
    $('#sheet').classList.toggle('has-foot', !!foot);
    $('#sheet').hidden = false;
    $('#scrim').hidden = false;
    inner.scrollTop = 0;
    hydratePhotos($('#sheet'));
    if (!S.sheetHist) { history.pushState({ sheet: 1 }, ''); S.sheetHist = true; }
    S.sheetOpen = true;
  }
  function closeSheet(fromPop = false) {
    if (!S.sheetOpen) return;
    S.sheetOpen = false;
    S.sheetKind = null;
    $('#sheet').hidden = true;
    $('#scrim').hidden = true;
    if (!fromPop && S.sheetHist) { S.ignorePop++; history.back(); }
    S.sheetHist = false;
  }
  async function dismissSheet() {
    if (S.sheetKind === 'form' && S.draft?.dirty) {
      if (!(await ask('Bỏ check-in này?', 'Những gì bạn vừa nhập sẽ không được lưu.', 'Bỏ đi'))) return;
    }
    if (S.sheetKind === 'form') S.draft = null;
    closeSheet();
  }

  /* ================= Check-in ================= */
  const OSM_CAT = [
    [/^(restaurant|fast_food|food_court|biergarten)$/, 'food'],
    [/^(cafe|ice_cream|bar|pub|bakery|tea|coffee)$/, 'cafe'],
    [/^(hotel|hostel|guest_house|motel|resort|apartment|chalet|camp_site)$/, 'stay'],
    [/^(attraction|museum|viewpoint|artwork|gallery|place_of_worship|monument|memorial|castle|ruins)$/, 'sight'],
    [/^(theme_park|zoo|aquarium|cinema|theatre|nightclub|amusement_arcade|water_park)$/, 'fun'],
    [/^(park|garden|beach|nature_reserve|peak|waterfall|wood|water|bay|cave_entrance)$/, 'nature'],
    [/^(marketplace|mall|supermarket|convenience)$/, 'shop']
  ];
  function guessCat(r) {
    if (!r) return null;
    for (const [re, cat] of OSM_CAT) if (re.test(r.type || '')) return cat;
    if (r.category === 'shop') return 'shop';
    if (r.category === 'natural') return 'nature';
    if (r.category === 'historic') return 'sight';
    return null;
  }

  function newDraft(lat, lng, opt = {}) {
    const g = Geo.guess(lat, lng) || {};
    return {
      id: uid(), journeyId: S.active ? S.active.id : null, lat, lng, name: opt.name || '', address: opt.address || '',
      cat: opt.cat || 'food', catTouched: !!opt.cat, rating: 0, price: null, tags: [], feedback: '', memory: '', mood: null,
      visitedAt: Date.now(), photos: [], removed: [], editing: false, dirty: false, locating: false, source: opt.source || 'map',
      province: g.province || '', country: g.country || '', cc: g.cc || '', regionSrc: g.cc ? 'guess' : 'none', moreOpen: false
    };
  }

  async function openCheckinForm(opt = {}) {
    if (opt.edit) {
      const c = S.checkins.find(x => x.id === opt.edit);
      const photos = await Promise.all(c.photoIds.map(async id => ({ id, url: await photoUrl(id), isNew: false })));
      S.draft = { ...c, tags: [...c.tags], photos, removed: [], editing: true, dirty: false, locating: false, catTouched: true, moreOpen: true, source: 'map' };
    } else {
      const fresh = S.me && Date.now() - S.me.t < 120000;
      let lat = opt.lat, lng = opt.lng, locating = false, source = 'map';
      if (lat == null) {
        if (fresh) { ({ lat, lng } = S.me); source = 'gps'; }
        else { const c = map.getCenter(); lat = c.lat; lng = c.lng; locating = geoOk; }
      }
      const d = S.draft = newDraft(lat, lng, { ...opt, source });
      d.locating = locating;
      if (locating) {
        locateOnce(false).then(me => {
          if (S.draft !== d) return;
          d.locating = false;
          if (me) { setDraftPos(me.lat, me.lng, 'gps'); return; }
          renderLocBox();
          fillFromGeocoder();
        });
      } else fillFromGeocoder();
    }
    renderForm();
  }

  function setDraftPos(lat, lng, source) {
    const d = S.draft;
    const g = Geo.guess(lat, lng) || {};
    Object.assign(d, { lat, lng, source, address: '', province: g.province || '', country: g.country || '', cc: g.cc || '', regionSrc: g.cc ? 'guess' : 'none' });
    renderLocBox();
    renderPrev();
    syncRegionInputs();
    fillFromGeocoder();
  }

  async function fillFromGeocoder() {
    const d = S.draft;
    if (!d) return;
    const r = await reverse(d.lat, d.lng);
    if (!r || S.draft !== d) return;
    const a = r.address || {};
    if (!d.address) d.address = [a.road && (a.house_number ? a.house_number + ' ' : '') + a.road, a.suburb || a.quarter || a.village || a.town, a.city_district || a.county].filter(Boolean).slice(0, 2).join(', ');
    if (d.regionSrc !== 'manual') {
      const reg = Geo.fromAddress(a, d.lat, d.lng);
      if (reg) Object.assign(d, { province: reg.province, country: reg.country, cc: reg.cc, regionSrc: 'geo' });
    }
    if (!d.editing) {
      const poi = r.name && r.name !== a.road ? r.name : '';
      if (!d.name && poi) { d.name = poi; const n = $('#fName'); if (n && !n.value) n.value = poi; }
      const gc = guessCat(r);
      if (gc && !d.catTouched && poi) { d.cat = gc; $$('#fCat .chip').forEach(x => x.classList.toggle('is-on', x.dataset.cat === gc)); }
    }
    renderLocBox();
    syncRegionInputs();
  }

  function renderLocBox() {
    const d = S.draft, el = $('#locBox');
    if (!d || !el) return;
    const where = whereText(d);
    el.innerHTML = `
      <span class="ic">${icon(d.locating ? 'locate' : 'pin')}</span>
      <span class="t">
        <b>${d.locating ? 'Đang lấy vị trí của bạn…' : esc(d.address || (d.source === 'gps' ? 'Vị trí hiện tại' : 'Vị trí đã chọn'))}</b>
        <small>${where ? esc(where) : `${d.lat.toFixed(5)}, ${d.lng.toFixed(5)}`}</small>
      </span>
      <button type="button" class="btn line sm" data-act="pick">Đổi</button>`;
  }
  function syncRegionInputs() {
    const d = S.draft;
    const p = $('#fProvince'), c = $('#fCountry');
    if (!d || !p) return;
    if (document.activeElement !== p) p.value = d.province || '';
    if (document.activeElement !== c) c.value = d.country || '';
  }
  /** Gợi ý nơi đã từng ghé trong bán kính 80 m để khỏi gõ lại. */
  function renderPrev() {
    const d = S.draft, el = $('#fPrev');
    if (!d || !el) return;
    const near = d.editing ? [] : places().filter(p => hav([p.lat, p.lng], [d.lat, d.lng]) < 80).slice(0, 3);
    el.hidden = !near.length;
    el.innerHTML = near.length ? `Bạn từng ghé gần đây: ${near.map(p => `<button type="button" class="chip" data-prev="${p.latest.id}">${catOf(p).e} ${esc(p.name)}</button>`).join('')}` : '';
  }

  function renderForm() {
    const d = S.draft;
    const extra = [d.price != null ? PRICES[d.price] : '', d.tags.length ? `${d.tags.length} thẻ` : '', d.mood ? MOOD[d.mood].e : ''].filter(Boolean).join(' · ');
    openSheet(`
      <div class="sheet-head">
        <h2>${d.editing ? 'Sửa check-in' : 'Check-in'}</h2>
        <button type="button" class="icon-btn" data-act="close" aria-label="Đóng">${icon('x')}</button>
      </div>
      <div class="loc-box" id="locBox"></div>
      <div class="field">
        <span class="label">Ảnh <span class="hint">· tối đa 12 ảnh, tự nén cho nhẹ</span></span>
        <div class="photo-grid" id="fPhotos"></div>
      </div>
      <div class="field">
        <label for="fName">Tên địa điểm</label>
        <input class="input" id="fName" value="${esc(d.name)}" placeholder="VD: Quán bún bò cô Ba" maxlength="100" autocomplete="off">
        <div class="prev-row" id="fPrev" hidden></div>
      </div>
      <div class="field">
        <span class="label">Loại địa điểm</span>
        <div class="chips" id="fCat">${CATS.map(c => `<button type="button" class="chip ${d.cat === c.id ? 'is-on' : ''}" data-cat="${c.id}">${c.e} ${c.label}</button>`).join('')}</div>
      </div>
      <div class="field">
        <span class="label">Bạn chấm mấy tim?</span>
        <div class="rate-input" id="fRate">${[1, 2, 3, 4, 5].map(i => `<button type="button" data-rate="${i}" class="${i <= d.rating ? 'on' : ''}" aria-label="${i} tim">${icon('heart')}</button>`).join('')}</div>
      </div>
      <div class="field">
        <label for="fFeedback">Feedback về địa điểm</label>
        <textarea class="input" id="fFeedback" placeholder="Món nào ngon, giá ra sao, phục vụ thế nào, có nên quay lại không…">${esc(d.feedback)}</textarea>
      </div>
      <div class="field">
        <label for="fMemory">Kỉ niệm ở đây</label>
        <textarea class="input script" id="fMemory" placeholder="Điều mình muốn nhớ mãi…">${esc(d.memory)}</textarea>
      </div>
      <details class="more" id="fMore" ${d.moreOpen ? 'open' : ''}>
        <summary>Thêm chi tiết <small id="fMoreSum">${esc(extra || 'giá, thẻ, tâm trạng, giờ')}</small>${icon('down')}</summary>
        <div class="in">
          <div class="field">
            <span class="label">Mức giá</span>
            <div class="chips" id="fPrice">${PRICES.map((p, i) => `<button type="button" class="chip ${d.price === i ? 'is-on' : ''}" data-price="${i}">${p}</button>`).join('')}</div>
          </div>
          <div class="field">
            <span class="label">Nhận xét nhanh</span>
            <div class="chips" id="fTags">${TAGS.map(t => `<button type="button" class="chip ${d.tags.includes(t) ? 'is-on' : ''}" data-tag="${esc(t)}">${esc(t)}</button>`).join('')}</div>
          </div>
          <div class="field">
            <span class="label">Tâm trạng</span>
            <div class="mood-input" id="fMood">${MOODS.map(m => `<button type="button" data-mood="${m.v}" class="${d.mood === m.v ? 'on' : ''}" aria-label="${m.l}" title="${m.l}">${m.e}</button>`).join('')}</div>
          </div>
          <div class="field">
            <label for="fTime">Thời gian</label>
            <input class="input" type="datetime-local" id="fTime" value="${toLocalInput(d.visitedAt)}">
          </div>
          <div class="field">
            <label for="fProvince">Tỉnh / thành phố <span class="hint">· tự nhận theo vị trí, sửa nếu chưa đúng</span></label>
            <input class="input" id="fProvince" list="vnUnits" value="${esc(d.province)}" placeholder="VD: Lâm Đồng" autocomplete="off">
          </div>
          <div class="field">
            <label for="fCountry">Quốc gia</label>
            <input class="input" id="fCountry" value="${esc(d.country)}" placeholder="VD: Việt Nam" autocomplete="off">
          </div>
        </div>
      </details>
    `, `
      <button type="button" class="btn ghost" data-act="close">Huỷ</button>
      <button type="button" class="btn primary" data-act="save">${icon('check')}${d.editing ? 'Lưu thay đổi' : 'Lưu check-in'}</button>
    `, 'form');
    renderLocBox();
    renderPrev();
    renderFormPhotos();
    bindForm();
  }

  function renderFormPhotos() {
    const d = S.draft, el = $('#fPhotos');
    if (!el) return;
    el.innerHTML = d.photos.map((p, i) => `
      <div class="ph">${p.busy ? '<span class="busy">Đang nén…</span>' : `<img src="${p.url}" alt="">`}
        <button type="button" class="rm" data-rm="${i}" aria-label="Bỏ ảnh">${icon('x')}</button></div>`).join('')
      + (d.photos.length < 12 ? `<label class="add" for="fFile">${icon('camera')}<span>Thêm ảnh</span><input type="file" id="fFile" accept="image/*" multiple></label>` : '');
    const f = $('#fFile');
    if (f) f.onchange = e => addPhotos([...e.target.files]);
  }
  async function addPhotos(files) {
    const d = S.draft;
    files = files.slice(0, 12 - d.photos.length);
    if (!files.length) return;
    d.dirty = true;
    const slots = files.map(() => ({ id: uid(), busy: true, isNew: true }));
    d.photos.push(...slots);
    renderFormPhotos();
    for (let i = 0; i < files.length; i++) {
      const { blob, thumb } = await processImage(files[i]);
      if (S.draft !== d) return;
      Object.assign(slots[i], { blob, thumb, url: URL.createObjectURL(thumb), busy: false });
      renderFormPhotos();
    }
  }

  function bindForm() {
    const d = S.draft;
    const mark = () => { d.dirty = true; };
    $('#fName').oninput = e => { d.name = e.target.value; mark(); };
    $('#fFeedback').oninput = e => { d.feedback = e.target.value; mark(); };
    $('#fMemory').oninput = e => { d.memory = e.target.value; mark(); };
    $('#fTime').onchange = e => { const t = new Date(e.target.value).getTime(); if (!isNaN(t)) d.visitedAt = t; mark(); };
    $('#fMore').ontoggle = e => { d.moreOpen = e.target.open; };
    $('#fProvince').onchange = e => {
      const v = e.target.value.trim();
      const unit = Geo.vnUnit(v);
      if (unit) Object.assign(d, { province: unit, country: 'Việt Nam', cc: 'VN' });
      else d.province = v;
      d.regionSrc = 'manual';
      syncRegionInputs(); renderLocBox(); mark();
    };
    $('#fCountry').onchange = e => {
      const v = e.target.value.trim();
      if (Geo.norm(v) === 'viet nam' || Geo.norm(v) === 'vietnam') Object.assign(d, { country: 'Việt Nam', cc: 'VN' });
      else if (v !== d.country) Object.assign(d, { country: v, cc: '' });
      d.regionSrc = 'manual';
      syncRegionInputs(); renderLocBox(); mark();
    };

    $('#sheetInner').onclick = e => {
      const b = e.target.closest('button');
      if (!b) return;
      if (b.dataset.cat) { d.cat = b.dataset.cat; d.catTouched = true; $$('#fCat .chip').forEach(x => x.classList.toggle('is-on', x === b)); mark(); }
      else if (b.dataset.rate) {
        const v = +b.dataset.rate;
        d.rating = d.rating === v ? 0 : v;
        $$('#fRate button').forEach(x => x.classList.toggle('on', +x.dataset.rate <= d.rating)); mark();
      } else if (b.dataset.price) {
        const v = +b.dataset.price;
        d.price = d.price === v ? null : v;
        $$('#fPrice .chip').forEach(x => x.classList.toggle('is-on', +x.dataset.price === d.price)); mark();
      } else if (b.dataset.tag) {
        const t = b.dataset.tag;
        d.tags = d.tags.includes(t) ? d.tags.filter(x => x !== t) : [...d.tags, t];
        b.classList.toggle('is-on'); mark();
      } else if (b.dataset.mood) {
        const v = +b.dataset.mood;
        d.mood = d.mood === v ? null : v;
        $$('#fMood button').forEach(x => x.classList.toggle('on', +x.dataset.mood === d.mood)); mark();
      } else if (b.dataset.rm) {
        const p = d.photos.splice(+b.dataset.rm, 1)[0];
        if (p && !p.isNew) d.removed.push(p.id);
        renderFormPhotos(); mark();
      } else if (b.dataset.prev) {
        const c = S.checkins.find(x => x.id === b.dataset.prev);
        Object.assign(d, { name: c.name, cat: c.cat, catTouched: true, price: c.price });
        $('#fName').value = c.name;
        $$('#fCat .chip').forEach(x => x.classList.toggle('is-on', x.dataset.cat === c.cat));
        $$('#fPrice .chip').forEach(x => x.classList.toggle('is-on', +x.dataset.price === d.price));
        mark();
      } else if (b.dataset.act === 'pick') startPick();
      else if (b.dataset.act === 'close') dismissSheet();
    };
    $('#sheetFoot').onclick = e => {
      const act = e.target.closest('[data-act]')?.dataset.act;
      if (act === 'save') saveCheckin();
      else if (act === 'close') dismissSheet();
    };
  }

  async function saveCheckin() {
    const d = S.draft;
    if (d.photos.some(p => p.busy)) { toast('Đợi ảnh nén xong một chút nhé'); return; }
    if (!d.name.trim()) {
      const n = $('#fName');
      n.scrollIntoView({ block: 'center' });
      n.focus();
      toast('Hãy đặt tên cho địa điểm');
      return;
    }
    for (const p of d.photos.filter(p => p.isNew)) {
      await DB.put('photos', { id: p.id, checkinId: d.id, blob: p.blob, thumb: p.thumb, createdAt: Date.now() });
    }
    for (const id of d.removed) { await DB.del('photos', id); dropUrls(id); }
    const now = Date.now();
    const c = {
      id: d.id, journeyId: d.journeyId, lat: d.lat, lng: d.lng, name: d.name.trim(), address: d.address,
      province: d.province, country: d.country, cc: d.cc, regionSrc: d.regionSrc,
      cat: d.cat, rating: d.rating, price: d.price, tags: d.tags, feedback: d.feedback.trim(), memory: d.memory.trim(),
      mood: d.mood, photoIds: d.photos.map(p => p.id), visitedAt: d.visitedAt,
      createdAt: d.createdAt || now, updatedAt: now
    };
    if (d.editing && d.demo) c.demo = true;
    await DB.put('checkins', c);
    const i = S.checkins.findIndex(x => x.id === c.id);
    if (i >= 0) S.checkins[i] = c; else S.checkins.push(c);
    S.checkins.sort((a, b) => b.visitedAt - a.visitedAt);
    S.draft = null;
    closeSheet();
    bump();
    toast(d.editing ? 'Đã cập nhật check-in' : `Đã check-in ${c.name}${c.province ? ' · ' + c.province : ''}`);
    if (!d.editing && S.view === 'map') {
      const z = Math.max(map.getZoom(), 16);
      map.flyTo(offsetCenter([c.lat, c.lng], z), z, { duration: .7 });
    }
  }

  /* --- chọn vị trí trên bản đồ --- */
  function pickNew() {
    const c = map.getCenter();
    S.draft = newDraft(c.lat, c.lng);
    S.draft.fromPick = true;
    startPick();
  }
  function startPick() {
    const d = S.draft;
    S.picking = true;
    closeSheet();
    go('map');
    if (!d.fromPick || map.getZoom() < 14) {
      if (d.fromPick) toast('Phóng to để đặt ghim cho chính xác');
      else map.setView([d.lat, d.lng], Math.max(map.getZoom(), 16));
    }
    $('#pickCross').hidden = false;
    $('#pickBar').hidden = false;
    renderRec();
    renderScope();
  }
  function endPick(ok) {
    S.picking = false;
    $('#pickCross').hidden = true;
    $('#pickBar').hidden = true;
    renderRec();
    renderScope();
    const d = S.draft;
    if (!d) return;
    const fresh = d.fromPick;
    d.fromPick = false;
    if (!ok && fresh) { S.draft = null; return; }
    renderForm();
    if (ok) {
      const c = map.getCenter();
      d.dirty = true;
      setDraftPos(c.lat, c.lng, 'map');
    }
  }

  /* ================= Chi tiết check-in ================= */
  function openDetail(id) {
    const c = S.checkins.find(x => x.id === id);
    if (!c) return;
    const cat = catOf(c);
    const j = c.journeyId && S.journeys.find(x => x.id === c.journeyId);
    const mood = MOOD[c.mood];
    const others = (placeOf(id)?.visits || []).filter(v => v.id !== id);
    const where = whereText(c);
    openSheet(`
      ${c.photoIds.length ? `<div class="carousel">${c.photoIds.map((p, i) => `<button type="button" data-lb="${i}"><img data-photo="${p}" data-size="full" alt=""></button>`).join('')}</div>` : ''}
      <div class="detail-title">
        ${where ? `<span class="where">${icon('pin')}${esc(where)}</span>` : ''}
        <h2>${esc(c.name)} ${c.demo ? '<span class="badge beige">Mẫu</span>' : ''}</h2>
        <span class="sub">${esc(c.address || `${c.lat.toFixed(5)}, ${c.lng.toFixed(5)}`)}</span>
      </div>
      <div class="kv">
        <div><span>Loại</span><b>${cat.e} ${cat.label}</b></div>
        <div><span>Đánh giá</span><b>${c.rating ? hearts(c.rating) : 'Chưa chấm'}</b></div>
        <div><span>Giá</span><b>${c.price != null ? PRICES[c.price] : '—'}</b></div>
        <div><span>Ngày</span><b>${dDate(c.visitedAt)}</b></div>
        <div><span>Giờ</span><b>${dTime(c.visitedAt)}</b></div>
        <div><span>Tâm trạng</span><b>${mood ? `${mood.e} ${mood.l}` : '—'}</b></div>
      </div>
      ${c.tags.length ? `<div class="tags">${c.tags.map(t => `<span class="tag">${esc(t)}</span>`).join('')}</div>` : ''}
      ${c.feedback ? `<p class="sec-label">Feedback</p><p class="fb-box">${esc(c.feedback)}</p>` : ''}
      ${c.memory ? `<p class="sec-label">Kỉ niệm</p><blockquote class="quote">${esc(c.memory)}</blockquote>` : ''}
      ${j ? `<button type="button" class="top-item" data-journey="${j.id}"><span class="rank">${icon('route')}</span><span class="t"><b>${esc(j.name)}</b><small>Thuộc hành trình · ${km(j.distance)}</small></span>${icon('chevron')}</button>` : ''}
      ${others.length ? `<p class="sec-label">Những lần ghé khác · ${others.length}</p><div class="mini-list">${others.map(v => `
        <button type="button" class="top-item" data-ci="${v.id}"><span class="rank">${icon('clock')}</span><span class="t"><b>${dDate(v.visitedAt)} · ${dTime(v.visitedAt)}</b><small>${v.rating ? '♥'.repeat(v.rating) + ' · ' : ''}${esc(v.feedback || v.memory || 'Không có ghi chú')}</small></span>${icon('chevron')}</button>`).join('')}</div>` : ''}
      <div class="actions-row">
        <button type="button" class="btn soft" data-act="show">${icon('map')}Trên bản đồ</button>
        <a class="btn line" href="https://www.google.com/maps/dir/?api=1&destination=${c.lat},${c.lng}" target="_blank" rel="noopener">${icon('nav')}Chỉ đường</a>
      </div>
    `, `
      <button type="button" class="btn ghost icon-only" data-act="del" aria-label="Xoá">${icon('trash')}</button>
      <button type="button" class="btn ghost" data-act="again">${icon('plus')}Ghé lại</button>
      <button type="button" class="btn primary" data-act="edit">${icon('edit')}Sửa</button>
    `, 'detail');
    $('#sheetInner').onclick = e => {
      const b = e.target.closest('button');
      if (!b) return;
      if (b.dataset.lb != null) openLightbox(c.photoIds, +b.dataset.lb, c.name);
      else if (b.dataset.journey) openJourney(b.dataset.journey);
      else if (b.dataset.ci) openDetail(b.dataset.ci);
      else if (b.dataset.act === 'show') { closeSheet(); go('map'); const z = Math.max(map.getZoom(), 16.5); map.flyTo(offsetCenter([c.lat, c.lng], z), z, { duration: .8 }); }
    };
    $('#sheetFoot').onclick = async e => {
      const act = e.target.closest('[data-act]')?.dataset.act;
      if (act === 'edit') openCheckinForm({ edit: c.id });
      else if (act === 'again') {
        S.draft = Object.assign(newDraft(c.lat, c.lng, { name: c.name, cat: c.cat, address: c.address }), { price: c.price, province: c.province, country: c.country, cc: c.cc, regionSrc: c.regionSrc || 'none' });
        renderForm();
      } else if (act === 'del') {
        if (!(await ask('Xoá check-in này?', `“${c.name}” và ${c.photoIds.length} ảnh sẽ bị xoá vĩnh viễn.`))) return;
        await deleteCheckin(c.id);
        closeSheet();
        toast('Đã xoá check-in');
      }
    };
  }

  async function deleteCheckin(id) {
    const c = S.checkins.find(x => x.id === id);
    for (const p of c.photoIds) { await DB.del('photos', p); dropUrls(p); }
    await DB.del('checkins', id);
    S.checkins = S.checkins.filter(x => x.id !== id);
    bump();
  }

  /* ================= Lightbox ================= */
  function openLightbox(ids, start = 0, cap = '') {
    const lb = $('#lightbox'), tr = $('#lbTrack');
    tr.innerHTML = ids.map(id => `<figure><img data-photo="${id}" data-size="full" alt=""></figure>`).join('');
    $('#lbCap').textContent = typeof cap === 'function' ? cap(start) : cap;
    lb.hidden = false;
    hydratePhotos(tr);
    requestAnimationFrame(() => { tr.scrollLeft = start * tr.clientWidth; });
    tr.onscroll = () => {
      if (typeof cap !== 'function') return;
      $('#lbCap').textContent = cap(Math.round(tr.scrollLeft / tr.clientWidth));
    };
  }
  function photoCaption(pid) {
    const c = S.checkins.find(x => x.photoIds.includes(pid));
    return c ? `${c.name} · ${dDate(c.visitedAt)}` : '';
  }

  /* ================= Hình thu nhỏ lộ trình ================= */
  function routeSvg(j, w = 92, h = 92) {
    const pts = j.points;
    const cs = jCheckins(j.id);
    const all = pts.map(p => [p[0], p[1]]).concat(cs.map(c => [c.lat, c.lng]));
    if (all.length < 2) return `<svg viewBox="0 0 ${w} ${h}"><circle cx="${w / 2}" cy="${h / 2}" r="5" fill="#EC7FA2"/></svg>`;
    const lat0 = all.reduce((a, p) => a + p[0], 0) / all.length;
    const k = Math.cos(lat0 * Math.PI / 180);
    const xs = all.map(p => p[1] * k), ys = all.map(p => -p[0]);
    const minX = Math.min(...xs), maxX = Math.max(...xs), minY = Math.min(...ys), maxY = Math.max(...ys);
    const span = Math.max(maxX - minX, maxY - minY) || 1e-6;
    const padd = 12, s = (w - padd * 2) / span;
    const ox = padd + ((w - padd * 2) - (maxX - minX) * s) / 2, oy = padd + ((h - padd * 2) - (maxY - minY) * s) / 2;
    const P = p => [(ox + (p[1] * k - minX) * s).toFixed(1), (oy + (-p[0] - minY) * s).toFixed(1)];
    const lines = segLines(pts).map(l => `<polyline points="${l.map(p => P(p).join(',')).join(' ')}" fill="none" stroke="#EC7FA2" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"/>`).join('');
    const dots = cs.map(c => { const [x, y] = P([c.lat, c.lng]); return `<circle cx="${x}" cy="${y}" r="3.6" fill="#fff" stroke="#C24E78" stroke-width="2"/>`; }).join('');
    const st = pts.length ? P(pts[0]) : null;
    return `<svg viewBox="0 0 ${w} ${h}">${lines}${dots}${st ? `<circle cx="${st[0]}" cy="${st[1]}" r="4" fill="#C24E78"/>` : ''}</svg>`;
  }

  /* ================= Nhật ký: mọi thứ theo ngày ================= */
  function renderDiary() {
    const q = S.diaryQuery.trim().toLowerCase();
    const regs = [...groupBy(S.checkins, rKey)].sort((a, b) => b[1].length - a[1].length);
    if (S.diaryReg !== 'all' && !regs.some(([rk]) => rk === S.diaryReg)) S.diaryReg = 'all';
    $('#diaryFilters').innerHTML = [`<button type="button" class="chip ${S.diaryReg === 'all' ? 'is-on' : ''}" data-reg="all">Tất cả</button>`]
      .concat(regs.map(([rk, l]) => `<button type="button" class="chip ${S.diaryReg === rk ? 'is-on' : ''}" data-reg="${esc(rk)}">${l[0].cc && l[0].cc !== 'VN' ? flagOf(l[0]) + ' ' : ''}${esc(rLabel(l[0]))} · ${l.length}</button>`)).join('');

    const el = $('#diaryList');
    if (!S.checkins.length && !S.journeys.length) {
      el.innerHTML = emptyState('book', 'Nhật ký còn trống', 'Mỗi lần check-in hay ghi một hành trình, nó sẽ hiện ở đây theo từng ngày.', true);
      return;
    }
    const inReg = x => S.diaryReg === 'all' || (x && rKey(x) === S.diaryReg);
    const cs = S.checkins.filter(c => inReg(c) && (!q || [c.name, c.address, c.province, c.country, c.feedback, c.memory, ...c.tags].join(' ').toLowerCase().includes(q)));
    const js = S.journeys.filter(j => inReg(S.diaryReg === 'all' ? null : jRegion(j)) && (!q || [j.name, j.note].join(' ').toLowerCase().includes(q) || cs.some(c => c.journeyId === j.id)));

    if (S.diaryMode === 'album') {
      const ids = cs.flatMap(c => c.photoIds);
      el.innerHTML = ids.length
        ? `<div class="gallery">${ids.map((p, i) => `<button type="button" data-lb="${i}"><img data-photo="${p}" alt=""></button>`).join('')}</div><p class="foot-note">${ids.length} ảnh từ ${cs.filter(c => c.photoIds.length).length} lần check-in</p>`
        : emptyState('image', 'Chưa có ảnh', 'Thêm ảnh khi check-in để album đầy dần lên.');
      el.dataset.ids = JSON.stringify(ids);
      hydratePhotos(el);
      return;
    }

    const days = new Map();
    const day = t => { const k = dayKey(t); if (!days.has(k)) days.set(k, { t, cs: [], js: [] }); return days.get(k); };
    cs.forEach(c => day(c.visitedAt).cs.push(c));
    js.forEach(j => day(j.startedAt).js.push(j));
    if (!days.size) { el.innerHTML = emptyState('search', 'Không tìm thấy', 'Thử bỏ bớt bộ lọc hoặc tìm bằng từ khác.'); return; }

    el.innerHTML = [...days.entries()].sort((a, b) => (a[0] < b[0] ? 1 : -1)).map(([, D]) => {
      const d = new Date(D.t);
      const regsOfDay = [...new Set(D.cs.map(rLabel).concat(D.js.map(j => { const r = jRegion(j); return r ? rLabel(r) : null; }).filter(Boolean)))];
      D.cs.sort((a, b) => a.visitedAt - b.visitedAt);
      D.js.sort((a, b) => a.startedAt - b.startedAt);
      return `
        <section class="day">
          <header class="day-head">
            <span class="dnum">${pad(d.getDate())}</span>
            <span class="dd"><strong>${WEEKDAYS[d.getDay()]}</strong><small>Tháng ${d.getMonth() + 1}, ${d.getFullYear()}</small></span>
            ${regsOfDay.slice(0, 2).map(r => `<span class="badge">${esc(r)}</span>`).join('')}
          </header>
          ${D.js.map(j => {
            const jc = jCheckins(j.id);
            return `<button type="button" class="jstrip" data-journey="${j.id}">
              <span class="thumb">${routeSvg(j, 54, 54)}</span>
              <span><span class="ttl">${esc(j.name)} ${j.status !== 'done' ? '<span class="badge">Đang ghi</span>' : ''}${j.demo ? ' <span class="badge beige">Mẫu</span>' : ''}</span>
                <span class="nums"><span>${icon('route')}${km(j.distance)}</span><span>${icon('clock')}${dur(elapsed(j))}</span><span>${icon('pin')}${jc.length}</span></span></span>
              ${icon('chevron')}
            </button>`;
          }).join('')}
          ${D.cs.length ? `<div class="rail">${D.cs.map(c => {
            const cat = catOf(c);
            const ph = c.photoIds.slice(0, 3);
            const more = c.photoIds.length - 3;
            return `<div class="stop"><span class="time">${dTime(c.visitedAt)}</span>
              <button type="button" class="card mcard" data-ci="${c.id}">
                ${ph.length ? `<span class="photos">${ph.map((p, i) => `<span class="${i === 2 && more > 0 ? 'more' : ''}" data-more="+${more}"><img data-photo="${p}" alt=""></span>`).join('')}</span>` : ''}
                <span class="body">
                  <span class="top">
                    <span class="cat">${cat.e}</span>
                    <span class="t"><span class="ttl">${esc(c.name)}</span><span class="sub">${cat.label}${c.price != null ? ' · ' + PRICES[c.price] : ''}${c.rating ? ' · ' + '♥'.repeat(c.rating) : ''}${c.demo ? ' · mẫu' : ''}</span></span>
                    ${c.mood ? `<span class="mood" title="${MOOD[c.mood].l}">${MOOD[c.mood].e}</span>` : ''}
                  </span>
                  ${c.memory ? `<span class="memo">${esc(c.memory)}</span>` : ''}
                  ${c.feedback ? `<span class="fb">${esc(c.feedback)}</span>` : ''}
                </span>
              </button></div>`;
          }).join('')}</div>` : ''}
        </section>`;
    }).join('');
    hydratePhotos(el);
  }

  function openJourney(id) {
    const j = S.journeys.find(x => x.id === id);
    if (!j) return;
    const cs = jCheckins(id).sort((a, b) => a.visitedAt - b.visitedAt);
    const ms = elapsed(j);
    const speed = ms > 60000 ? (j.distance / 1000) / (ms / 3600000) : 0;
    const photos = cs.flatMap(c => c.photoIds);
    const reg = jRegion(j);
    openSheet(`
      <div class="sheet-head">
        <h2>${esc(j.name)}</h2>
        <button type="button" class="icon-btn" data-act="close" aria-label="Đóng">${icon('x')}</button>
      </div>
      <p class="foot-note left" style="margin-top:-10px">${reg ? esc(whereText(reg)) + ' · ' : ''}${WEEKDAYS[new Date(j.startedAt).getDay()]}, ${dDate(j.startedAt)} · ${dTime(j.startedAt)}${j.endedAt ? `–${dTime(j.endedAt)}` : ''}${j.demo ? ' · <span class="badge beige">Mẫu</span>' : ''}</p>
      <div class="jstats">
        <div><b>${km(j.distance)}</b><span>Quãng đường</span></div>
        <div><b>${dur(ms)}</b><span>Thời gian di chuyển</span></div>
        <div><b>${nf1.format(speed)} km/h</b><span>Tốc độ trung bình</span></div>
        <div><b>${nf.format(Math.round(j.distance / STRIDE_M))}</b><span>Bước chân (ước tính)</span></div>
      </div>
      ${j.note ? `<blockquote class="quote">${esc(j.note)}</blockquote>` : ''}
      ${photos.length ? `<p class="sec-label">Ảnh trong hành trình · ${photos.length}</p><div class="gallery">${photos.slice(0, 9).map((p, i) => `<button type="button" data-lb="${i}"><img data-photo="${p}" alt=""></button>`).join('')}</div>` : ''}
      <p class="sec-label">Điểm đã check-in · ${cs.length}</p>
      ${cs.length ? `<div class="mini-list">${cs.map((c, i) => `
        <button type="button" class="top-item" data-ci="${c.id}">
          <span class="rank">${i + 1}</span>
          <span class="t"><b>${esc(c.name)}</b><small>${catOf(c).e} ${dTime(c.visitedAt)}${c.rating ? ' · ' + '♥'.repeat(c.rating) : ''}</small></span>
          ${icon('chevron')}
        </button>`).join('')}</div>` : '<p class="foot-note left">Chưa check-in điểm nào trong hành trình này.</p>'}
    `, `
      <button type="button" class="btn ghost icon-only" data-act="del" aria-label="Xoá hành trình">${icon('trash')}</button>
      <button type="button" class="btn ghost" data-act="rename">${icon('edit')}Sửa</button>
      <button type="button" class="btn primary" data-act="map">${icon('map')}Xem lộ trình</button>
    `, 'journey');
    $('#sheetInner').onclick = e => {
      const b = e.target.closest('button');
      if (!b) return;
      if (b.dataset.ci) openDetail(b.dataset.ci);
      else if (b.dataset.lb != null) openLightbox(photos, +b.dataset.lb, i => photoCaption(photos[i]));
      else if (b.dataset.act === 'close') closeSheet();
    };
    $('#sheetFoot').onclick = async e => {
      const act = e.target.closest('[data-act]')?.dataset.act;
      if (act === 'map') { closeSheet(); focusJourney(j.id); }
      else if (act === 'rename') editJourney(j);
      else if (act === 'del') {
        if (j.status !== 'done') { toast('Hãy kết thúc hành trình trước khi xoá'); return; }
        if (!(await ask('Xoá hành trình này?', 'Lộ trình sẽ bị xoá. Các check-in và ảnh vẫn được giữ lại.'))) return;
        closeSheet();
        await deleteJourney(j.id);
      }
    };
  }

  function editJourney(j) {
    openSheet(`
      <div class="sheet-head"><h2>Sửa hành trình</h2></div>
      <div class="field"><label for="jName">Tên hành trình</label><input class="input" id="jName" value="${esc(j.name)}" maxlength="80"></div>
      <div class="field"><label for="jNote">Cảm nhận</label><textarea class="input script" id="jNote" placeholder="Hôm nay mình đã…">${esc(j.note)}</textarea></div>
    `, `
      <button type="button" class="btn ghost" data-act="back">Huỷ</button>
      <button type="button" class="btn primary" data-act="save">Lưu</button>
    `, 'jedit');
    $('#sheetFoot').onclick = async e => {
      const act = e.target.closest('[data-act]')?.dataset.act;
      if (act === 'save') {
        j.name = $('#jName').value.trim() || j.name;
        j.note = $('#jNote').value.trim();
        await DB.put('journeys', j);
        drawRoutes();
        bump();
        toast('Đã lưu');
        openJourney(j.id);
      } else if (act === 'back') openJourney(j.id);
    };
  }

  function focusJourney(id) {
    const j = S.journeys.find(x => x.id === id);
    if (!j) return;
    S.focusId = id;
    L_focus.clearLayers();
    const lines = segLines(j.points);
    const pts = j.points.map(p => [p[0], p[1]]).concat(jCheckins(id).map(c => [c.lat, c.lng]));
    if (lines.length) {
      L.polyline(lines, { color: '#fff', weight: 10, opacity: .9, lineCap: 'round', lineJoin: 'round' }).addTo(L_focus);
      L.polyline(lines, { color: '#C24E78', weight: 5, opacity: 1, lineCap: 'round', lineJoin: 'round' }).addTo(L_focus);
      const first = j.points[0], last = j.points[j.points.length - 1];
      L.circleMarker([first[0], first[1]], { radius: 7, color: '#fff', weight: 3, fillColor: '#5FA98A', fillOpacity: 1 }).bindTooltip('Xuất phát', { className: 'route-tip' }).addTo(L_focus);
      L.circleMarker([last[0], last[1]], { radius: 7, color: '#fff', weight: 3, fillColor: '#C24E78', fillOpacity: 1 }).bindTooltip('Kết thúc', { className: 'route-tip' }).addTo(L_focus);
    }
    $('#focusTitle').textContent = `${j.name} · ${km(j.distance)}`;
    $('#focusBanner').hidden = false;
    $('#view-map').classList.add('has-focus');
    go('map');
    S.follow = false;
    if (pts.length) map.fitBounds(L.latLngBounds(pts).pad(0.2), { maxZoom: 17, paddingTopLeft: L.point(16, 150), paddingBottomRight: padBR() });
  }
  function unfocus() {
    S.focusId = null;
    L_focus.clearLayers();
    $('#focusBanner').hidden = true;
    $('#view-map').classList.remove('has-focus');
  }

  /* ================= Địa điểm: sổ tay theo tỉnh/thành ================= */
  function renderPlaces() {
    const P = places();
    const counts = {};
    P.forEach(p => { counts[p.cat] = (counts[p.cat] || 0) + 1; });
    const opts = [{ id: 'all', label: `Tất cả · ${P.length}` }, { id: 'fav', label: '♥ Yêu thích' }, { id: 'again', label: 'Đáng quay lại' }]
      .concat(CATS.filter(c => counts[c.id]).map(c => ({ id: c.id, label: `${c.e} ${c.label} · ${counts[c.id]}` })));
    $('#placeFilters').innerHTML = opts.map(o => `<button type="button" class="chip ${S.placeCat === o.id ? 'is-on' : ''}" data-pcat="${o.id}">${o.label}</button>`).join('');

    const el = $('#placeList');
    if (!P.length) {
      el.innerHTML = emptyState('bookmark', 'Chưa có địa điểm nào', 'Quán ăn, quán cà phê, chỗ chơi bạn check-in sẽ được xếp gọn theo từng tỉnh thành ở đây.', true);
      return;
    }
    const q = S.placeQuery.trim().toLowerCase();
    const list = P.filter(p => {
      if (S.placeCat === 'fav' && p.rating < 5) return false;
      if (S.placeCat === 'again' && !p.tags.includes('Đáng quay lại')) return false;
      if (!['all', 'fav', 'again'].includes(S.placeCat) && p.cat !== S.placeCat) return false;
      return !q || [p.name, p.province, p.country, p.feedback, catOf(p).label, ...p.tags].join(' ').toLowerCase().includes(q);
    });
    if (!list.length) { el.innerHTML = emptyState('search', 'Không tìm thấy', 'Thử bỏ bớt bộ lọc hoặc tìm bằng từ khác.'); return; }
    const sorters = {
      recent: (a, b) => b.latest.visitedAt - a.latest.visitedAt,
      rating: (a, b) => b.rating - a.rating || b.visits.length - a.visits.length || b.latest.visitedAt - a.latest.visitedAt,
      name: (a, b) => a.name.localeCompare(b.name, 'vi')
    };
    list.sort(sorters[S.placeSort]);

    const countries = [...groupBy(list, cKey)].sort((a, b) => (a[0] === 'VN' ? -1 : b[0] === 'VN' ? 1 : b[1].length - a[1].length));
    el.innerHTML = countries.map(([, cl]) => {
      const regs = [...groupBy(cl, rKey)].sort((a, b) => b[1].length - a[1].length);
      return `
        <p class="country-label"><span>${flagOf(cl[0])}</span>${esc(cLabel(cl[0]))} · ${regs.length} ${cl[0].cc === 'VN' ? 'tỉnh thành' : 'vùng'}</p>
        ${regs.map(([rk, rl]) => {
          const rated = rl.filter(p => p.rating);
          const avg = rated.length ? rated.reduce((a, p) => a + p.rating, 0) / rated.length : 0;
          return `<details class="card grp" open>
            <summary>
              <span class="nm"><b>${esc(rLabel(rl[0]))}</b><small>${rl.length} địa điểm${avg ? ` · ♥ ${nf1.format(avg)}` : ''}</small></span>
              <button type="button" class="to-map" data-rk="${esc(rk)}" aria-label="Xem ${esc(rLabel(rl[0]))} trên bản đồ">${icon('map')}</button>
              <span class="caret">${icon('down')}</span>
            </summary>
            <div class="rows">${rl.map(p => `
              <button type="button" class="prow" data-ci="${p.latest.id}">
                <span class="th">${p.photoId ? `<img data-photo="${p.photoId}" alt="">` : catOf(p).e}</span>
                <span class="t">
                  <span class="ttl">${esc(p.name)}</span>
                  <span class="sub">${catOf(p).e} ${catOf(p).label}${p.price != null ? ' · ' + PRICES[p.price] : ''}${p.visits.length > 1 ? ` · ghé ${p.visits.length} lần` : ''}${p.demo ? ' · mẫu' : ''}</span>
                  ${p.feedback ? `<span class="fb">${esc(p.feedback)}</span>` : (p.tags.length ? `<span class="fb">${esc(p.tags.slice(0, 3).join(' · '))}</span>` : '')}
                </span>
                ${p.rating ? `<span class="score">${p.rating}${icon('heart')}</span>` : ''}
              </button>`).join('')}</div>
          </details>`;
        }).join('')}`;
    }).join('');
    hydratePhotos(el);
  }

  /* ================= Dấu chân: hộ chiếu + thống kê ================= */
  function vnSvg(groups) {
    const minLng = 101.8, maxLat = 23.7, k = 0.96, s = 20;
    const W = (115.3 - minLng) * k * s, H = (maxLat - 7.9) * s;
    const X = lng => ((lng - minLng) * k * s).toFixed(1), Y = lat => ((maxLat - lat) * s).toFixed(1);
    const d = VN_SHAPE.map(poly => 'M' + poly.map(p => `${X(p[1])} ${Y(p[0])}`).join('L') + 'Z').join('');
    const isle = (pts, label, lx, ly) => pts.map(([la, ln]) => `<circle cx="${X(ln)}" cy="${Y(la)}" r="1.5" fill="#EC7FA2"/>`).join('')
      + `<text x="${X(lx)}" y="${Y(ly)}" font-size="8.5" font-weight="600" fill="#8E747B" text-anchor="middle">${label}</text>`;
    const dots = groups.map(([, l]) => {
      const [la, ln] = centroid(l);
      return `<circle cx="${X(ln)}" cy="${Y(la)}" r="${(7 + Math.sqrt(l.length) * 2.4).toFixed(1)}" fill="#EC7FA2" fill-opacity=".28"/><circle cx="${X(ln)}" cy="${Y(la)}" r="4.6" fill="#C24E78" stroke="#fff" stroke-width="1.6"/>`;
    }).join('');
    return `<svg viewBox="0 0 ${W.toFixed(0)} ${H.toFixed(0)}" role="img" aria-label="Bản đồ Việt Nam với các tỉnh thành đã đến">
      <path d="${d}" fill="#fff" fill-opacity=".8" stroke="#EC7FA2" stroke-width="1.3" stroke-linejoin="round"/>
      ${isle([[16.5, 111.6], [16.9, 112.3], [16.2, 112.0], [15.8, 111.3]], 'Hoàng Sa', 112.6, 15.1)}
      ${isle([[10.4, 114.3], [9.7, 113.7], [8.9, 113.3], [11.0, 114.6], [8.7, 112.0], [9.9, 114.6]], 'Trường Sa', 113.4, 7.6)}
      ${dots}
    </svg>`;
  }

  function renderMe() {
    const el = $('#meBody');
    const J = S.journeys, C = S.checkins, P = places();
    if (!J.length && !C.length) {
      el.innerHTML = `
        <section class="card passport">
          <div class="l"><span class="lbl">Việt Nam của bạn</span><span class="big">0<small>/${Geo.UNITS.length}</small></span><span class="cap">tỉnh thành đã đặt chân</span>
            <button type="button" class="btn soft sm" data-act="demo">${icon('sparkle')}Xem thử dữ liệu mẫu</button></div>
          ${vnSvg([])}
        </section>` + dataPanel();
      return;
    }
    const vn = P.filter(p => p.cc === 'VN');
    const vnGroups = [...groupBy(vn, rKey)];
    const visited = new Map(vnGroups.map(([, l]) => [l[0].province, l.length]));
    const countries = [...groupBy(P, cKey)].sort((a, b) => b[1].length - a[1].length);
    const dist = J.reduce((a, j) => a + j.distance, 0);
    const moving = J.reduce((a, j) => a + elapsed(j), 0);
    const photos = C.reduce((a, c) => a + c.photoIds.length, 0);
    const days = new Set(C.map(c => dayKey(c.visitedAt)).concat(J.map(j => dayKey(j.startedAt)))).size;

    const byCat = CATS.map(c => ({ ...c, n: P.filter(x => x.cat === c.id).length })).filter(c => c.n).sort((a, b) => b.n - a.n);
    const maxCat = Math.max(1, ...byCat.map(c => c.n));
    const now = new Date();
    const months = [];
    for (let i = 5; i >= 0; i--) { const d = new Date(now.getFullYear(), now.getMonth() - i, 1); months.push({ y: d.getFullYear(), m: d.getMonth(), n: 0 }); }
    C.forEach(c => { const d = new Date(c.visitedAt); const mm = months.find(x => x.y === d.getFullYear() && x.m === d.getMonth()); if (mm) mm.n++; });
    const maxM = Math.max(1, ...months.map(m => m.n));
    const rated = P.filter(p => p.rating);
    const avg = rated.length ? rated.reduce((a, p) => a + p.rating, 0) / rated.length : 0;
    const dist5 = [5, 4, 3, 2, 1].map(r => ({ r, n: rated.filter(p => p.rating === r).length }));
    const maxR = Math.max(1, ...dist5.map(d => d.n));
    const top = rated.slice().sort((a, b) => b.rating - a.rating || b.visits.length - a.visits.length || b.photos - a.photos).slice(0, 5);
    const longest = J.filter(j => j.status === 'done').sort((a, b) => b.distance - a.distance)[0];
    const moodN = MOODS.map(m => ({ ...m, n: C.filter(c => c.mood === m.v).length }));
    const again = P.filter(p => p.tags.includes('Đáng quay lại')).length;

    const regionBlock = r => {
      const units = Geo.UNITS.filter(u => u.region === r);
      const n = units.filter(u => visited.has(u.name)).length;
      return `<h3><span>Miền ${r}</span><span>${n}/${units.length}</span></h3>
        <div class="chips">${units.map(u => visited.has(u.name)
          ? `<button type="button" class="stamp on" data-rk="VN|${esc(u.name)}">${esc(u.name)}<i>${visited.get(u.name)}</i></button>`
          : `<span class="stamp">${esc(u.name)}</span>`).join('')}</div>`;
    };

    el.innerHTML = `
      <section class="card passport">
        <div class="l">
          <span class="lbl">Việt Nam của bạn</span>
          <span class="big">${visited.size}<small>/${Geo.UNITS.length}</small></span>
          <span class="cap">tỉnh thành đã đặt chân</span>
          <div class="meter"><span style="width:${(visited.size / Geo.UNITS.length) * 100}%"></span></div>
          <span class="mini">${countries.length} quốc gia · ${P.length} địa điểm · ${kmNum(dist)} km</span>
          <button type="button" class="btn primary sm" data-act="home">${icon('vn')}Mở bản đồ chữ S</button>
        </div>
        ${vnSvg(vnGroups)}
      </section>

      <section class="card panel stamps">
        <h2>Sổ đóng dấu <small>chạm dấu để mở bản đồ</small></h2>
        ${['Bắc', 'Trung', 'Nam'].map(regionBlock).join('')}
        ${countries.some(([ck]) => ck !== 'VN') ? `<h3><span>Nước ngoài</span><span>${countries.filter(([ck]) => ck !== 'VN').length}</span></h3>
          <div class="chips">${countries.filter(([ck]) => ck !== 'VN').map(([ck, l]) => `<button type="button" class="stamp on" data-cc="${esc(ck)}">${flagOf(l[0])} ${esc(cLabel(l[0]))}<i>${l.length}</i></button>`).join('')}</div>` : ''}
      </section>

      <div class="tiles">
        <div class="card tile"><b>${kmNum(dist)}</b><span>km đã ghi</span></div>
        <div class="card tile"><b>${nf.format(Math.round(dist / STRIDE_M))}</b><span>Bước chân</span></div>
        <div class="card tile"><b>${J.length}</b><span>Hành trình</span></div>
        <div class="card tile"><b>${C.length}</b><span>Lượt check-in</span></div>
        <div class="card tile"><b>${photos}</b><span>Ảnh kỉ niệm</span></div>
        <div class="card tile"><b>${days}</b><span>Ngày khám phá</span></div>
      </div>

      ${byCat.length ? `<section class="card panel">
        <h2>Bạn hay đi đâu nhất? <small>${byCat[0].e} ${byCat[0].label}</small></h2>
        <div class="bars">${byCat.map(c => `<div class="bar-row"><span class="name">${c.e} ${c.label}</span><span class="track"><span class="fill" style="width:${(c.n / maxCat) * 100}%"></span></span><span class="n">${c.n}</span></div>`).join('')}</div>
      </section>` : ''}

      <section class="card panel">
        <h2>Check-in theo tháng <small>6 tháng gần đây</small></h2>
        <div class="cols">${months.map(m => `<div class="col"><div class="stack" style="height:100%"><span class="v ${m.n ? '' : 'zero'}" style="height:${m.n ? Math.max(6, (m.n / maxM) * 100) : 2}%"></span>${m.n ? `<span class="num" style="bottom:${Math.max(6, (m.n / maxM) * 100)}%">${m.n}</span>` : ''}</div><span class="m">T${m.m + 1}</span></div>`).join('')}</div>
        ${moving ? `<p class="foot-note left">Tổng thời gian di chuyển đã ghi: <b>${dur(moving)}</b></p>` : ''}
      </section>

      ${rated.length ? `<section class="card panel">
        <h2>Feedback của bạn <small>${rated.length} nơi đã chấm</small></h2>
        <div class="rate-row"><span class="rate-big">${nf1.format(avg)}<small> /5</small></span>${hearts(Math.round(avg))}</div>
        <div class="bars">${dist5.map(d => `<div class="bar-row"><span class="name">${'♥'.repeat(d.r)}</span><span class="track"><span class="fill" style="width:${(d.n / maxR) * 100}%"></span></span><span class="n">${d.n}</span></div>`).join('')}</div>
        ${again ? `<p class="foot-note left">${again} nơi bạn đánh dấu “Đáng quay lại”.</p>` : ''}
      </section>` : ''}

      ${top.length ? `<section class="card panel">
        <h2>Nơi bạn thương nhất</h2>
        <div class="top-list">${top.map((p, i) => `
          <button type="button" class="top-item" data-ci="${p.latest.id}">
            <span class="rank">${i + 1}</span>
            ${p.photoId ? `<img data-photo="${p.photoId}" alt="">` : `<span class="ph">${catOf(p).e}</span>`}
            <span class="t"><b>${esc(p.name)}</b><small>${'♥'.repeat(p.rating)} · ${esc(rLabel(p))}${p.visits.length > 1 ? ` · ghé ${p.visits.length} lần` : ''}</small></span>
          </button>`).join('')}</div>
      </section>` : ''}

      ${longest ? `<section class="card panel">
        <h2>Hành trình dài nhất</h2>
        <button type="button" class="jstrip" data-journey="${longest.id}">
          <span class="thumb">${routeSvg(longest, 54, 54)}</span>
          <span><span class="ttl">${esc(longest.name)}</span><span class="nums"><span>${icon('route')}${km(longest.distance)}</span><span>${icon('clock')}${dur(longest.activeMs)}</span><span>${icon('calendar')}${dDate(longest.startedAt)}</span></span></span>
          ${icon('chevron')}
        </button>
      </section>` : ''}

      ${moodN.some(m => m.n) ? `<section class="card panel">
        <h2>Tâm trạng trên đường</h2>
        <div class="moods">${moodN.map(m => `<div><span class="e">${m.e}</span><b>${m.n}</b><span>${m.l}</span></div>`).join('')}</div>
      </section>` : ''}

      ${dataPanel()}
    `;
    hydratePhotos(el);
  }

  function dataPanel() {
    const hasDemo = S.checkins.some(c => c.demo) || S.journeys.some(j => j.demo);
    return `
      <section class="card panel">
        <h2>Dữ liệu của bạn</h2>
        <p class="foot-note left">Mọi thứ được lưu ngay trên điện thoại này, không gửi lên máy chủ nào. Hãy sao lưu định kỳ để không mất kỉ niệm.</p>
        <div class="data-actions">
          <button type="button" class="btn line sm" data-act="export">${icon('download')}Sao lưu</button>
          <button type="button" class="btn line sm" data-act="import">${icon('upload')}Khôi phục</button>
          ${hasDemo
            ? `<button type="button" class="btn ghost sm" data-act="cleardemo">Xoá dữ liệu mẫu</button>`
            : `<button type="button" class="btn ghost sm" data-act="demo">${icon('sparkle')}Thêm dữ liệu mẫu</button>`}
          <button type="button" class="btn ghost sm" data-act="wipe" style="color:var(--danger)">${icon('trash')}Xoá tất cả</button>
        </div>
      </section>`;
  }

  function emptyState(ic, title, text, withDemo = false) {
    return `<div class="empty-state">
      <span class="art">${icon(ic)}</span>
      <h3>${title}</h3><p>${text}</p>
      ${withDemo && !S.checkins.length ? `<div class="row"><button type="button" class="btn soft sm" data-act="demo">${icon('sparkle')}Xem thử dữ liệu mẫu</button></div>` : ''}
    </div>`;
  }

  /* ================= Sao lưu ================= */
  const blobToData = b => new Promise(r => { const f = new FileReader(); f.onload = () => r(f.result); f.readAsDataURL(b); });
  const dataToBlob = async u => (await fetch(u)).blob();

  async function exportData() {
    toast('Đang chuẩn bị bản sao lưu…');
    const photos = await DB.all('photos');
    const out = {
      app: 'dau-chan-hong', version: 2, exportedAt: new Date().toISOString(),
      journeys: await DB.all('journeys'), checkins: await DB.all('checkins'),
      photos: await Promise.all(photos.map(async p => ({ id: p.id, checkinId: p.checkinId, createdAt: p.createdAt, demo: p.demo, blob: await blobToData(p.blob), thumb: await blobToData(p.thumb || p.blob) })))
    };
    const blob = new Blob([JSON.stringify(out)], { type: 'application/json' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = `dau-chan-hong-${dayKey(Date.now())}.json`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(a.href), 4000);
    toast('Đã tạo tệp sao lưu');
  }
  async function importData(file) {
    try {
      const data = JSON.parse(await file.text());
      if (data.app !== 'dau-chan-hong') throw new Error('not ours');
      for (const p of data.photos || []) await DB.put('photos', { ...p, blob: await dataToBlob(p.blob), thumb: await dataToBlob(p.thumb) });
      for (const c of data.checkins || []) await DB.put('checkins', c);
      for (const j of data.journeys || []) await DB.put('journeys', j.status === 'done' ? j : { ...j, status: 'done', resumedAt: null, endedAt: j.endedAt || Date.now() });
      await loadAll();
      backfillRegions();
      toast(`Đã khôi phục ${(data.checkins || []).length} check-in, ${(data.journeys || []).length} hành trình`);
    } catch {
      toast('Tệp này không phải bản sao lưu của Dấu Chân Hồng');
    }
  }
  async function addDemo() {
    toast('Đang thêm dữ liệu mẫu…');
    const d = await Demo.build(uid);
    for (const p of d.photos) await DB.put('photos', p);
    for (const c of d.checkins) await DB.put('checkins', c);
    for (const j of d.journeys) await DB.put('journeys', j);
    await loadAll();
    if (S.view === 'map') showS();
    toast('Đã thêm các chuyến đi mẫu dọc chữ S');
  }
  async function clearDemo() {
    for (const c of S.checkins.filter(c => c.demo)) {
      for (const p of c.photoIds) { await DB.del('photos', p); dropUrls(p); }
      await DB.del('checkins', c.id);
    }
    for (const j of S.journeys.filter(j => j.demo)) await DB.del('journeys', j.id);
    unfocus();
    await loadAll();
    toast('Đã xoá dữ liệu mẫu');
  }
  async function wipeAll() {
    if (!(await ask('Xoá toàn bộ dữ liệu?', 'Tất cả hành trình, check-in và ảnh sẽ bị xoá khỏi máy này. Không thể hoàn tác.', 'Xoá hết'))) return;
    if (S.active) { stopWatch(); wake(false); S.active = null; }
    await Promise.all(['journeys', 'checkins', 'photos'].map(s => DB.clear(s)));
    urlCache.forEach(u => URL.revokeObjectURL(u));
    urlCache.clear();
    unfocus();
    await loadAll();
    toast('Đã xoá toàn bộ dữ liệu');
  }

  /* ================= Tìm kiếm trên bản đồ ================= */
  async function search(q) {
    const box = $('#searchResults');
    q = q.trim();
    if (!q) { box.hidden = true; return; }
    const local = places().filter(p => [p.name, p.province].join(' ').toLowerCase().includes(q.toLowerCase())).slice(0, 4);
    const renderLocal = () => local.map(p => `<li><button type="button" data-place="${p.latest.id}"><b>${catOf(p).e} ${esc(p.name)}</b><small>Đã check-in · ${esc(rLabel(p))} · ${dDate(p.latest.visitedAt)}</small></button></li>`).join('');
    box.hidden = false;
    box.innerHTML = renderLocal() + '<li class="empty">Đang tìm trên bản đồ…</li>';
    let remote;
    try {
      const c = map.getCenter();
      const res = await fetch(`https://nominatim.openstreetmap.org/search?format=jsonv2&q=${encodeURIComponent(q)}&accept-language=vi&limit=8&viewbox=${c.lng - 0.5},${c.lat + 0.5},${c.lng + 0.5},${c.lat - 0.5}&bounded=0`);
      remote = res.ok ? await res.json() : [];
    } catch { remote = null; }
    box.innerHTML = renderLocal() + (remote === null
      ? '<li class="empty">Không kết nối được dịch vụ tìm kiếm.</li>'
      : remote.length
        ? remote.map(r => `<li><button type="button" data-lat="${r.lat}" data-lng="${r.lon}" data-name="${esc(r.name || r.display_name.split(',')[0])}" data-addr="${esc(r.display_name)}"><b>${esc(r.name || r.display_name.split(',')[0])}</b><small>${esc(r.display_name)}</small></button></li>`).join('')
        : (local.length ? '' : '<li class="empty">Không tìm thấy địa điểm phù hợp.</li>'));
  }
  function clearTemp() { if (tempMarker) { map.removeLayer(tempMarker); tempMarker = null; } }
  function showSearchResult(lat, lng, name, addr) {
    $('#searchResults').hidden = true;
    clearTemp();
    const el = document.createElement('div');
    el.className = 'pop';
    el.innerHTML = `<b>${esc(name)}</b><small>${esc(addr.split(',').slice(1, 4).join(','))}</small><br><button type="button">＋ Check-in tại đây</button>`;
    el.querySelector('button').onclick = () => {
      clearTemp();
      openCheckinForm({ lat, lng, name, address: addr.split(',').slice(0, 2).join(','), fromMap: true });
    };
    tempMarker = L.marker([lat, lng], { icon: L.divIcon({ className: 'mk', html: '<div class="pin"><span>🔎</span></div>', iconSize: [36, 36], iconAnchor: [18, 45], popupAnchor: [0, -42] }), zIndexOffset: 900 })
      .addTo(map).bindPopup(el, { closeButton: false });
    S.follow = false;
    map.flyTo(offsetCenter([lat, lng], 17), 17, { duration: .8 });
    setTimeout(() => tempMarker && tempMarker.openPopup(), 900);
  }

  /* ================= Điều hướng & vẽ lại ================= */
  function go(view) {
    closeFab();
    S.view = view;
    $$('.view').forEach(v => v.classList.toggle('is-active', v.dataset.view === view));
    $$('.tab[data-go]').forEach(t => t.classList.toggle('is-on', t.dataset.go === view));
    if (view === 'map') { map.invalidateSize(); renderScope(); }
    renderRec();
    renderView();
  }
  function renderView() {
    if (S.view === 'diary') renderDiary();
    else if (S.view === 'places') renderPlaces();
    else if (S.view === 'me') renderMe();
  }
  /** Dữ liệu vừa đổi: tính lại danh sách địa điểm và vẽ lại mọi thứ đang hiển thị. */
  function bump() {
    S.rev++;
    S._places = null;
    drawPins();
    renderScope();
    renderRec();
    renderView();
  }

  async function loadAll() {
    S.journeys = (await DB.all('journeys')).sort((a, b) => b.startedAt - a.startedAt);
    S.checkins = (await DB.all('checkins')).sort((a, b) => b.visitedAt - a.visitedAt);
    S.checkins.forEach(c => { c.tags = c.tags || []; c.photoIds = c.photoIds || []; c.name = c.name || 'Điểm check-in'; });
    const live = S.journeys.find(j => j.status !== 'done');
    if (live && !S.active) {
      // Mở lại app giữa chừng: khôi phục ở trạng thái tạm dừng để không ghi nhầm khoảng trống.
      if (live.status === 'active') {
        const lastT = live.points.length ? live.points[live.points.length - 1][2] : live.resumedAt;
        live.activeMs += Math.max(0, (lastT || 0) - (live.resumedAt || lastT || 0));
        live.status = 'paused'; live.resumedAt = null;
        await DB.put('journeys', live);
      }
      S.active = live;
      toast('Hành trình đang dở. Bấm ▶ để ghi tiếp.');
    } else if (S.active) {
      S.active = S.journeys.find(j => j.id === S.active.id) || null;
    }
    drawRoutes();
    drawActive();
    bump();
  }

  function onboarding() {
    DB.setMeta('onboarded', true).catch(() => {});
    const li = (ic, t, s) => `<li><span class="ic">${icon(ic)}</span><span><b>${t}</b>${s}</span></li>`;
    openSheet(`
      <div class="onboard">
        <img class="logo" src="icons/icon.svg" alt="">
        <h2>Dấu Chân Hồng</h2>
        <p>Bản đồ chữ S của riêng bạn. Mỗi nơi đã đến là một chiếc ghim.</p>
        <ul>
          ${li('map', 'Bản đồ', 'Ghim gom theo quốc gia, tỉnh thành. Phóng to để thấy từng quán.')}
          ${li('book', 'Nhật ký', 'Hành trình, ảnh và kỉ niệm xếp theo từng ngày.')}
          ${li('bookmark', 'Địa điểm', 'Quán ăn, chỗ chơi kèm feedback, gọn theo tỉnh thành.')}
          ${li('award', 'Dấu chân', 'Đã đi bao nhiêu trong 34 tỉnh thành, bao nhiêu km.')}
        </ul>
      </div>
    `, `
      <button type="button" class="btn ghost" data-act="demo">Xem thử dữ liệu mẫu</button>
      <button type="button" class="btn primary" data-act="go">Bắt đầu</button>
    `, 'onboard');
    $('#sheetFoot').onclick = async e => {
      const act = e.target.closest('[data-act]')?.dataset.act;
      if (!act) return;
      closeSheet();
      if (act === 'demo') await addDemo();
    };
  }

  /* ================= Gắn sự kiện ================= */
  function bind() {
    $$('.tab[data-go]').forEach(t => t.onclick = () => go(t.dataset.go));
    $('#btnAdd').onclick = () => ($('#fabMenu').hidden ? openFab() : closeFab());
    $('#fabScrim').onclick = closeFab;
    $('#fabMenu').onclick = e => { const b = e.target.closest('[data-act]'); if (b) { closeFab(); doAct(b.dataset.act); } };
    $('#scrim').onclick = dismissSheet;
    window.addEventListener('popstate', () => {
      if (S.ignorePop > 0) { S.ignorePop--; return; }
      if (!$('#lightbox').hidden) { $('#lightbox').hidden = true; if (S.sheetOpen) history.pushState({ sheet: 1 }, ''); return; }
      if (S.sheetOpen) { if (S.sheetKind === 'form') S.draft = null; closeSheet(true); }
      S.sheetHist = false;
    });

    $('#recBar').onclick = e => { const b = e.target.closest('[data-act]'); if (b) doAct(b.dataset.act); };
    $('#scopeCard').onclick = e => {
      const b = e.target.closest('button');
      if (!b || b.disabled) return;
      if (b.dataset.rk) flyRegion(b.dataset.rk);
      else if (b.dataset.cc) flyCountry(b.dataset.cc);
      else if (b.dataset.place) { const p = placeOf(b.dataset.place); if (p) flyPlace(p); }
      else if (b.dataset.act) doAct(b.dataset.act);
    };

    $('#btnHome').onclick = () => showS();
    $('#btnLocate').onclick = () => { S.follow = true; $('#btnLocate').classList.add('is-on'); locateOnce(true); };
    $('#btnLayers').onclick = () => {
      const m = $('#layerMenu');
      m.innerHTML = Object.entries(TILES).map(([k, t]) => `<button type="button" data-tile="${k}" class="${S.tile === k ? 'is-on' : ''}">${t.name}${S.tile === k ? icon('check') : ''}</button>`).join('');
      m.hidden = !m.hidden;
    };
    $('#layerMenu').onclick = e => {
      const b = e.target.closest('[data-tile]');
      if (!b) return;
      setTile(b.dataset.tile);
      DB.setMeta('tile', S.tile).catch(() => {});
      $('#layerMenu').hidden = true;
    };
    $('#focusClose').onclick = unfocus;
    $('#pickOk').onclick = () => endPick(true);
    $('#pickCancel').onclick = () => endPick(false);

    const si = $('#searchInput');
    $('#searchForm').onsubmit = e => { e.preventDefault(); si.blur(); search(si.value); };
    si.oninput = () => { $('#searchClear').hidden = !si.value; if (!si.value) $('#searchResults').hidden = true; };
    $('#searchClear').onclick = () => { si.value = ''; $('#searchClear').hidden = true; $('#searchResults').hidden = true; clearTemp(); };
    $('#searchResults').onclick = e => {
      const b = e.target.closest('button');
      if (!b) return;
      if (b.dataset.place) { $('#searchResults').hidden = true; const p = placeOf(b.dataset.place); if (p) flyPlace(p); }
      else showSearchResult(+b.dataset.lat, +b.dataset.lng, b.dataset.name, b.dataset.addr);
    };

    // Nhật ký
    $('#diaryList').onclick = e => {
      const b = e.target.closest('[data-ci],[data-lb],[data-journey]');
      if (!b) return;
      if (b.dataset.ci) openDetail(b.dataset.ci);
      else if (b.dataset.journey) openJourney(b.dataset.journey);
      else { const ids = JSON.parse($('#diaryList').dataset.ids || '[]'); openLightbox(ids, +b.dataset.lb, i => photoCaption(ids[i])); }
    };
    $('#diaryFilters').onclick = e => { const b = e.target.closest('[data-reg]'); if (b) { S.diaryReg = b.dataset.reg; renderDiary(); } };
    $('#diarySearch').oninput = e => { S.diaryQuery = e.target.value; renderDiary(); };
    $$('[data-diarymode]').forEach(b => b.onclick = () => {
      S.diaryMode = b.dataset.diarymode;
      $$('[data-diarymode]').forEach(x => x.classList.toggle('is-on', x === b));
      renderDiary();
    });

    // Địa điểm
    $('#placeList').onclick = e => {
      const m = e.target.closest('.to-map');
      if (m) { e.preventDefault(); go('map'); flyRegion(m.dataset.rk); return; }
      const b = e.target.closest('[data-ci]');
      if (b) openDetail(b.dataset.ci);
    };
    $('#placeFilters').onclick = e => { const b = e.target.closest('[data-pcat]'); if (b) { S.placeCat = b.dataset.pcat; renderPlaces(); } };
    $('#placeSearch').oninput = e => { S.placeQuery = e.target.value; renderPlaces(); };
    $('#placeSort').onchange = e => { S.placeSort = e.target.value; renderPlaces(); };

    // Dấu chân
    $('#meBody').onclick = e => {
      const b = e.target.closest('[data-ci],[data-journey],[data-rk],[data-cc]');
      if (!b) return;
      if (b.dataset.ci) openDetail(b.dataset.ci);
      else if (b.dataset.journey) openJourney(b.dataset.journey);
      else if (b.dataset.rk) { go('map'); flyRegion(b.dataset.rk); }
      else if (b.dataset.cc) { go('map'); flyCountry(b.dataset.cc); }
    };

    // Các nút hành động chung nằm trong trang (dữ liệu mẫu, sao lưu…)
    document.addEventListener('click', e => {
      const b = e.target.closest('.page [data-act]');
      if (b) doAct(b.dataset.act);
    });
    $('#fileImport').onchange = e => { const f = e.target.files[0]; if (f) importData(f); e.target.value = ''; };

    $('#lbClose').onclick = () => { $('#lightbox').hidden = true; };
    document.addEventListener('keydown', e => {
      if (e.key !== 'Escape') return;
      if (!$('#lightbox').hidden) $('#lightbox').hidden = true;
      else if (!$('#fabMenu').hidden) closeFab();
      else if (S.sheetOpen) dismissSheet();
    });

    setInterval(() => { if (S.active?.status === 'active' && S.view === 'map') renderRec(); }, 1000);
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible' && S.active?.status === 'active') wake(true);
      if (document.visibilityState === 'hidden') saveActive();
    });
    window.addEventListener('pagehide', saveActive);
    window.addEventListener('resize', () => map && map.invalidateSize());
  }

  /* ================= Khởi động ================= */
  async function init() {
    hydrateIcons();
    $('#vnUnits').innerHTML = Geo.UNITS.map(u => `<option value="${esc(u.name)}">`).join('');
    initMap();
    bind();
    try {
      await DB.open();
      const tile = await DB.getMeta('tile', 'voyager');
      if (tile !== S.tile) setTile(tile);
      await loadAll();
    } catch {
      toast('Trình duyệt chặn bộ nhớ cục bộ, dữ liệu sẽ không được lưu.');
      bump();
    }
    showS(false);
    let onboarded = false;
    try { onboarded = await DB.getMeta('onboarded', false); } catch { /* bỏ qua */ }
    if (!onboarded) onboarding();
    else if (navigator.permissions) {
      try {
        const p = await navigator.permissions.query({ name: 'geolocation' });
        if (p.state === 'granted') locateOnce(false);
      } catch { /* bỏ qua */ }
    }
    backfillRegions();
    if ('serviceWorker' in navigator && location.protocol !== 'file:') {
      navigator.serviceWorker.register('sw.js').catch(() => {});
    }
  }

  init();
})();
