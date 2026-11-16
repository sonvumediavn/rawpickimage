/* Dấu Chân Hồng — logic chính của app. */
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

  const TILES = {
    voyager: { name: 'Bản đồ phố', url: 'https://{s}.basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}{r}.png', opt: { subdomains: 'abcd', maxZoom: 20, attribution: '© <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> © <a href="https://carto.com/attributions">CARTO</a>' } },
    osm: { name: 'OpenStreetMap', url: 'https://tile.openstreetmap.org/{z}/{x}/{y}.png', opt: { maxZoom: 19, attribution: '© <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>' } },
    sat: { name: 'Vệ tinh', url: 'https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}', opt: { maxZoom: 19, attribution: 'Ảnh vệ tinh © Esri' } }
  };

  /* ================= Trạng thái ================= */
  const S = {
    view: 'map',
    journeys: [],
    checkins: [],
    active: null,        // hành trình đang ghi / tạm dừng
    me: null,            // { lat, lng, acc, t }
    watchId: null,
    follow: true,
    gpsMsg: '',
    gpsWarn: false,
    tile: 'voyager',
    showRoutes: false,
    focusId: null,
    memMode: 'timeline',
    memCat: 'all',
    memQuery: '',
    draft: null,
    picking: false,
    sheetOpen: false,
    sheetHist: false,
    sheetKind: null,
    wakeLock: null,
    lastSave: 0
  };

  /* ================= Tiện ích ================= */
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => [...r.querySelectorAll(s)];
  const uid = () => (crypto.randomUUID ? crypto.randomUUID() : Date.now().toString(36) + Math.random().toString(36).slice(2));
  const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  const pad = n => String(n).padStart(2, '0');
  const nf = new Intl.NumberFormat('vi-VN');
  const nf1 = new Intl.NumberFormat('vi-VN', { maximumFractionDigits: 1, minimumFractionDigits: 1 });

  function hav(a, b) {
    const R = 6371000, toR = Math.PI / 180;
    const dLat = (b[0] - a[0]) * toR, dLng = (b[1] - a[1]) * toR;
    const s = Math.sin(dLat / 2) ** 2 + Math.cos(a[0] * toR) * Math.cos(b[0] * toR) * Math.sin(dLng / 2) ** 2;
    return 2 * R * Math.asin(Math.sqrt(s));
  }
  const km = m => (m < 1000 ? `${Math.round(m)} m` : `${(m / 1000) < 100 ? nf1.format(m / 1000) : nf.format(Math.round(m / 1000))} km`);
  const kmNum = m => (m / 1000 < 100 ? nf1.format(m / 1000) : nf.format(Math.round(m / 1000)));
  function dur(ms, withSec = false) {
    const t = Math.max(0, Math.floor(ms / 1000));
    const h = Math.floor(t / 3600), m = Math.floor((t % 3600) / 60), s = t % 60;
    if (withSec) return h ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
    if (h) return `${h} giờ ${m ? m + ' phút' : ''}`.trim();
    return `${m} phút`;
  }
  const dDate = t => { const d = new Date(t); return `${pad(d.getDate())}/${pad(d.getMonth() + 1)}/${d.getFullYear()}`; };
  const dTime = t => { const d = new Date(t); return `${pad(d.getHours())}:${pad(d.getMinutes())}`; };
  const dayKey = t => { const d = new Date(t); return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`; };
  const toLocalInput = t => { const d = new Date(t); return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`; };
  const hearts = (n, size = '') => `<span class="hearts ${size}" aria-label="${n}/5 tim">${[1, 2, 3, 4, 5].map(i => icon('heart', i <= n ? 'on' : 'off')).join('')}</span>`;

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

  /* ================= Ảnh ================= */
  const urlCache = new Map();
  async function photoUrl(id, size = 'thumb') {
    const key = id + ':' + size;
    if (urlCache.has(key)) return urlCache.get(key);
    const p = await DB.get('photos', id);
    if (!p) return '';
    const blob = size === 'thumb' ? (p.thumb || p.blob) : (p.blob || p.thumb);
    const u = URL.createObjectURL(blob);
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
      if (img.src) return;
      img.src = await photoUrl(img.dataset.photo, img.dataset.size || 'thumb');
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
      return { blob, thumb };
    } catch {
      return { blob: file, thumb: file }; // định dạng trình duyệt không đọc được: giữ nguyên tệp
    }
  }

  /* ================= Bản đồ ================= */
  let map, tileLayer, meMarker, accCircle, activeLine, tempMarker;
  const L_checkins = L.layerGroup();
  const L_routes = L.layerGroup();
  const L_focus = L.layerGroup();

  function initMap() {
    map = L.map('map', { zoomControl: false, attributionControl: true, tap: true, preferCanvas: false })
      .setView([16.0544, 108.2022], 6); // Việt Nam
    setTile(S.tile);
    L_routes.addTo(map);
    L_focus.addTo(map);
    L_checkins.addTo(map);
    activeLine = L.polyline([], { color: '#E57F9D', weight: 6, opacity: .95, lineCap: 'round', lineJoin: 'round' }).addTo(map);

    map.on('dragstart', () => { S.follow = false; $('#btnLocate').classList.remove('is-on'); });
    map.on('contextmenu', e => { if (!S.picking) openCheckinForm({ lat: e.latlng.lat, lng: e.latlng.lng, fromMap: true }); });
    map.on('click', () => { $('#searchResults').hidden = true; });
  }

  function setTile(key) {
    if (tileLayer) map.removeLayer(tileLayer);
    const t = TILES[key];
    tileLayer = L.tileLayer(t.url, t.opt).addTo(map);
    $('#map').classList.toggle('is-sat', key === 'sat');
    S.tile = key;
  }

  function pinIcon(c) {
    const cat = CAT[c.cat] || CAT.other;
    return L.divIcon({
      className: 'pin-wrap',
      html: `<div class="pin ${c.rating >= 5 ? 'fav' : ''}"><span>${cat.e}</span></div>`,
      iconSize: [38, 38], iconAnchor: [19, 46], popupAnchor: [0, -40]
    });
  }

  function drawCheckins() {
    L_checkins.clearLayers();
    S.checkins.forEach(c => {
      const m = L.marker([c.lat, c.lng], { icon: pinIcon(c), title: c.name });
      const el = document.createElement('div');
      el.className = 'pop';
      el.innerHTML = `<b>${esc(c.name || 'Điểm check-in')}</b><small>${esc((CAT[c.cat] || CAT.other).label)} · ${dDate(c.visitedAt)}</small><div>${hearts(c.rating)}</div><button type="button">Xem chi tiết ›</button>`;
      el.querySelector('button').onclick = () => openDetail(c.id);
      m.bindPopup(el, { closeButton: false });
      m.addTo(L_checkins);
    });
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
    if (!S.showRoutes) return;
    S.journeys.filter(j => j.status === 'done' && j.points.length > 1).forEach(j => {
      L.polyline(segLines(j.points), { color: '#C2577A', weight: 4, opacity: .45, dashArray: '1 8', lineCap: 'round' })
        .bindTooltip(j.name, { sticky: true, className: 'route-tip' })
        .on('click', () => openJourney(j.id))
        .addTo(L_routes);
    });
  }

  function drawActive() {
    activeLine.setLatLngs(S.active ? segLines(S.active.points) : []);
  }

  function updateMe(lat, lng, acc) {
    S.me = { lat, lng, acc, t: Date.now() };
    if (!meMarker) {
      meMarker = L.marker([lat, lng], { icon: L.divIcon({ className: 'pin-wrap', html: '<div class="me-dot"></div>', iconSize: [18, 18], iconAnchor: [9, 9] }), interactive: false, zIndexOffset: 1000 }).addTo(map);
      accCircle = L.circle([lat, lng], { radius: acc, color: '#E57F9D', weight: 1, opacity: .4, fillColor: '#F8D9E2', fillOpacity: .25, interactive: false }).addTo(map);
    } else {
      meMarker.setLatLng([lat, lng]);
      accCircle.setLatLng([lat, lng]).setRadius(acc);
    }
  }

  function fitAll() {
    const pts = S.checkins.map(c => [c.lat, c.lng]);
    S.journeys.forEach(j => { if (j.points.length) pts.push([j.points[0][0], j.points[0][1]]); });
    if (pts.length) map.fitBounds(L.latLngBounds(pts).pad(0.2), { maxZoom: 15 });
  }

  /* ================= Định vị ================= */
  const geoOk = 'geolocation' in navigator;
  const GEO_OPT = { enableHighAccuracy: true, maximumAge: 3000, timeout: 20000 };

  function geoError(err) {
    const msgs = {
      1: 'Bạn chưa cho phép truy cập vị trí. Hãy bật quyền vị trí cho trang này trong cài đặt trình duyệt.',
      2: 'Không xác định được vị trí. Hãy ra chỗ thoáng hoặc bật GPS.',
      3: 'Lấy vị trí quá lâu. Đang thử lại…'
    };
    S.gpsMsg = msgs[err.code] || 'Không lấy được vị trí.';
    S.gpsWarn = true;
    renderTrack();
    if (err.code === 1) toast('Cần quyền truy cập vị trí');
  }

  function locateOnce(fly = true) {
    if (!geoOk) { toast('Trình duyệt không hỗ trợ định vị'); return Promise.resolve(null); }
    return new Promise(resolve => {
      navigator.geolocation.getCurrentPosition(pos => {
        const { latitude: lat, longitude: lng, accuracy } = pos.coords;
        updateMe(lat, lng, accuracy);
        if (fly) map.flyTo([lat, lng], Math.max(map.getZoom(), 16), { duration: .8 });
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
    if (S.follow) map.panTo([lat, lng], { animate: true });
    if (!j || j.status !== 'active') return;

    if (accuracy > 50) {
      S.gpsMsg = `Tín hiệu GPS yếu (±${Math.round(accuracy)} m), đang chờ chính xác hơn…`;
      S.gpsWarn = true;
      renderTrack();
      return;
    }
    S.gpsMsg = `GPS tốt · ±${Math.round(accuracy)} m`;
    S.gpsWarn = false;

    const now = pos.timestamp || Date.now();
    const last = j.points[j.points.length - 1];
    if (!last || last[3] !== j.seg) {
      j.points.push([+lat.toFixed(6), +lng.toFixed(6), now, j.seg]);
    } else {
      const d = hav(last, [lat, lng]);
      const dt = Math.max(1, (now - last[2]) / 1000);
      if (d < 8 || d / dt > 70) { renderTrack(); return; } // đứng yên hoặc nhảy điểm bất thường
      j.points.push([+lat.toFixed(6), +lng.toFixed(6), now, j.seg]);
      j.distance += d;
    }
    drawActive();
    renderTrack();
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
    startWatch();
    wake(true);
    renderTrack();
    toast('Bắt đầu ghi hành trình');
  }

  async function pauseJourney() {
    const j = S.active;
    j.activeMs = elapsed(j);
    j.resumedAt = null;
    j.status = 'paused';
    stopWatch();
    wake(false);
    await saveActive();
    renderTrack();
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
    renderTrack();
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
    renderTrack();
    openJourneySummary(j);
  }

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
      <button type="button" class="btn primary" data-act="save">Lưu hành trình</button>
    `, 'summary');
    $('#sheetFoot').onclick = async e => {
      const act = e.target.closest('[data-act]')?.dataset.act;
      if (act === 'save') {
        j.name = $('#jName').value.trim() || j.name;
        j.note = $('#jNote').value.trim();
        await DB.put('journeys', j);
        closeSheet();
        toast('Đã lưu hành trình');
        refresh();
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
    refresh();
    if (!silent) toast('Đã xoá hành trình');
  }

  function renderTrack() {
    const el = $('#trackCard');
    el.hidden = S.picking || S.view !== 'map';
    const j = S.active;
    if (!j) {
      el.dataset.key = '';
      const total = S.journeys.reduce((a, b) => a + b.distance, 0);
      el.innerHTML = `
        <div class="track-idle">
          <div class="txt">
            <h3>Sẵn sàng khám phá?</h3>
            <p>${S.journeys.length ? `${S.journeys.length} hành trình · ${km(total)} đã đi` : 'Bấm bắt đầu để ghi lại từng con đường bạn đi qua'}</p>
          </div>
          <button type="button" class="btn primary" data-act="start">${icon('play')}Bắt đầu</button>
        </div>`;
      return;
    }
    const ms = elapsed(j);
    const paused = j.status === 'paused';
    const speed = ms > 60000 ? (j.distance / 1000) / (ms / 3600000) : 0;
    const vals = [kmNum(j.distance), dur(ms, true), jCheckins(j.id).length, nf1.format(speed)];
    const key = [j.id, j.status, j.name, S.gpsMsg, S.gpsWarn].join('|');
    if (el.dataset.key === key) { // chỉ cập nhật số, giữ nguyên các nút
      $$('[data-v]', el).forEach((b, i) => { b.textContent = vals[i]; });
      return;
    }
    el.dataset.key = key;
    el.innerHTML = `
      <div class="track-live">
        <div class="head">
          <span class="rec-dot ${paused ? 'paused' : ''}"></span>
          <h3>${esc(j.name)}</h3>
          <span class="rec-label ${paused ? 'paused' : ''}">${paused ? 'Tạm dừng' : 'Đang ghi'}</span>
        </div>
        <div class="track-stats">
          <div><b data-v>${vals[0]}</b><span>km</span></div>
          <div><b data-v>${vals[1]}</b><span>thời gian</span></div>
          <div><b data-v>${vals[2]}</b><span>check-in</span></div>
          <div><b data-v>${vals[3]}</b><span>km/h TB</span></div>
        </div>
        ${S.gpsMsg && !paused ? `<p class="gps-note ${S.gpsWarn ? 'warn' : ''}">${esc(S.gpsMsg)} · giữ màn hình mở để ghi chính xác</p>` : ''}
        <div class="row">
          ${paused
            ? `<button type="button" class="btn soft" data-act="resume">${icon('play')}Tiếp tục</button>`
            : `<button type="button" class="btn soft" data-act="pause">${icon('pause')}Tạm dừng</button>`}
          <button type="button" class="btn primary" data-act="end">${icon('stop')}Kết thúc</button>
        </div>
      </div>`;
  }

  /* ================= Bottom sheet ================= */
  function openSheet(html, foot = '', kind = null) {
    S.sheetKind = kind;
    $('#sheetInner').innerHTML = html;
    const f = $('#sheetFoot');
    f.innerHTML = foot;
    f.hidden = !foot;
    f.onclick = null;
    $('#sheet').classList.toggle('has-foot', !!foot);
    $('#sheet').hidden = false;
    $('#scrim').hidden = false;
    $('#sheetInner').scrollTop = 0;
    hydrateIcons($('#sheet'));
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
    if (!fromPop && S.sheetHist) { S.sheetHist = false; history.back(); } else S.sheetHist = false;
  }
  async function scrimClose() {
    if (S.sheetKind === 'form' && S.draft?.dirty) {
      if (!(await ask('Bỏ check-in này?', 'Những gì bạn vừa nhập sẽ không được lưu.', 'Bỏ đi'))) return;
    }
    if (S.sheetKind === 'form') S.draft = null;
    closeSheet();
  }

  /* ================= Check-in form ================= */
  async function openCheckinForm(opt = {}) {
    if (opt.edit) {
      const c = S.checkins.find(x => x.id === opt.edit);
      const photos = await Promise.all(c.photoIds.map(async id => ({ id, url: await photoUrl(id), isNew: false })));
      S.draft = { ...c, tags: [...c.tags], photos, removed: [], editing: true, dirty: false, locating: false };
    } else {
      const now = Date.now();
      const fresh = S.me && now - S.me.t < 120000;
      let lat = opt.lat, lng = opt.lng, locating = false;
      if (lat == null) {
        if (fresh) ({ lat, lng } = S.me);
        else { const c = map.getCenter(); lat = c.lat; lng = c.lng; locating = geoOk; }
      }
      S.draft = {
        id: uid(), journeyId: S.active ? S.active.id : null, lat, lng, name: opt.name || '', address: opt.address || '',
        cat: opt.cat || 'food', rating: 0, price: null, tags: [], feedback: '', memory: '', mood: null,
        visitedAt: now, photos: [], removed: [], editing: false, dirty: false, locating,
        source: opt.fromMap ? 'map' : (lat === S.me?.lat ? 'gps' : 'map')
      };
      if (locating) {
        locateOnce(false).then(me => {
          if (!S.draft || S.draft.editing) return;
          S.draft.locating = false;
          if (me) { S.draft.lat = me.lat; S.draft.lng = me.lng; S.draft.source = 'gps'; }
          renderLocBox();
          reverseGeocode();
        });
      } else if (!opt.address) reverseGeocode();
    }
    renderForm();
  }

  function renderLocBox() {
    const d = S.draft, el = $('#locBox');
    if (!d || !el) return;
    el.innerHTML = `
      <span class="ic">${icon(d.locating ? 'locate' : 'pin')}</span>
      <span class="t">
        <b>${d.locating ? 'Đang lấy vị trí của bạn…' : esc(d.address || (d.source === 'gps' ? 'Vị trí hiện tại' : 'Vị trí đã chọn'))}</b>
        <small>${d.lat.toFixed(5)}, ${d.lng.toFixed(5)}</small>
      </span>
      <button type="button" class="btn line sm" data-act="pick">Đổi</button>`;
  }

  const geoCache = new Map();
  async function reverseGeocode() {
    const d = S.draft;
    if (!d) return;
    const key = d.lat.toFixed(4) + ',' + d.lng.toFixed(4);
    let r = geoCache.get(key);
    if (!r) {
      try {
        const res = await fetch(`https://nominatim.openstreetmap.org/reverse?format=jsonv2&lat=${d.lat}&lon=${d.lng}&zoom=18&addressdetails=1&accept-language=vi`);
        if (!res.ok) return;
        r = await res.json();
        geoCache.set(key, r);
      } catch { return; }
    }
    if (S.draft !== d) return;
    const a = r.address || {};
    d.address = [a.road && (a.house_number ? a.house_number + ' ' : '') + a.road, a.suburb || a.quarter || a.village, a.city || a.town || a.state].filter(Boolean).join(', ') || r.display_name || '';
    const poi = r.name && r.name !== a.road ? r.name : '';
    if (!d.name && poi) {
      d.name = poi;
      const n = $('#fName');
      if (n && !n.value) n.value = poi;
    }
    renderLocBox();
  }

  function renderForm() {
    const d = S.draft;
    openSheet(`
      <div class="sheet-head">
        <h2>${d.editing ? 'Sửa check-in' : 'Check-in'}</h2>
        <button type="button" class="icon-btn" data-act="close" aria-label="Đóng">${icon('x')}</button>
      </div>
      <div class="loc-box" id="locBox"></div>
      <div class="field">
        <label for="fName">Tên địa điểm</label>
        <input class="input" id="fName" value="${esc(d.name)}" placeholder="VD: Quán bún bò cô Ba" maxlength="100" autocomplete="off">
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
        <span class="label">Mức giá</span>
        <div class="chips" id="fPrice">${PRICES.map((p, i) => `<button type="button" class="chip ${d.price === i ? 'is-on' : ''}" data-price="${i}">${p}</button>`).join('')}</div>
      </div>
      <div class="field">
        <span class="label">Nhận xét nhanh</span>
        <div class="chips" id="fTags">${TAGS.map(t => `<button type="button" class="chip ${d.tags.includes(t) ? 'is-on' : ''}" data-tag="${esc(t)}">${esc(t)}</button>`).join('')}</div>
      </div>
      <div class="field">
        <label for="fFeedback">Feedback về địa điểm</label>
        <textarea class="input" id="fFeedback" placeholder="Món nào ngon, giá ra sao, phục vụ thế nào, có nên quay lại không…">${esc(d.feedback)}</textarea>
      </div>
      <div class="field">
        <label for="fMemory">Kỉ niệm ở đây</label>
        <textarea class="input script" id="fMemory" placeholder="Điều mình muốn nhớ mãi…">${esc(d.memory)}</textarea>
      </div>
      <div class="field">
        <span class="label">Tâm trạng</span>
        <div class="mood-input" id="fMood">${MOODS.map(m => `<button type="button" data-mood="${m.v}" class="${d.mood === m.v ? 'on' : ''}" aria-label="${m.l}" title="${m.l}">${m.e}</button>`).join('')}</div>
      </div>
      <div class="field">
        <span class="label">Ảnh <span class="hint">· tối đa 12 ảnh, tự nén cho nhẹ</span></span>
        <div class="photo-grid" id="fPhotos"></div>
      </div>
      <div class="field">
        <label for="fTime">Thời gian</label>
        <input class="input" type="datetime-local" id="fTime" value="${toLocalInput(d.visitedAt)}">
      </div>
    `, `
      <button type="button" class="btn ghost" data-act="close">Huỷ</button>
      <button type="button" class="btn primary" data-act="save">${icon('heart')}${d.editing ? 'Lưu thay đổi' : 'Lưu check-in'}</button>
    `, 'form');
    renderLocBox();
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
    const inner = $('#sheetInner');
    const mark = () => { d.dirty = true; };
    $('#fName').oninput = e => { d.name = e.target.value; mark(); };
    $('#fFeedback').oninput = e => { d.feedback = e.target.value; mark(); };
    $('#fMemory').oninput = e => { d.memory = e.target.value; mark(); };
    $('#fTime').onchange = e => { const t = new Date(e.target.value).getTime(); if (!isNaN(t)) d.visitedAt = t; mark(); };

    inner.onclick = async e => {
      const b = e.target.closest('button');
      if (!b) return;
      if (b.dataset.cat) { d.cat = b.dataset.cat; $$('#fCat .chip').forEach(x => x.classList.toggle('is-on', x === b)); mark(); }
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
      } else if (b.dataset.act === 'pick') startPick();
      else if (b.dataset.act === 'close') scrimClose();
    };
    $('#sheetFoot').onclick = e => {
      const act = e.target.closest('[data-act]')?.dataset.act;
      if (act === 'save') saveCheckin();
      else if (act === 'close') scrimClose();
    };
  }

  async function saveCheckin() {
    const d = S.draft;
    if (d.photos.some(p => p.busy)) { toast('Đợi ảnh nén xong một chút nhé'); return; }
    if (!d.name.trim()) {
      const n = $('#fName');
      n.focus();
      n.placeholder = 'Đặt tên cho địa điểm này nhé';
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
      cat: d.cat, rating: d.rating, price: d.price, tags: d.tags, feedback: d.feedback.trim(), memory: d.memory.trim(),
      mood: d.mood, photoIds: d.photos.map(p => p.id), visitedAt: d.visitedAt,
      createdAt: d.createdAt || now, updatedAt: now, demo: d.editing ? d.demo : undefined
    };
    await DB.put('checkins', c);
    const i = S.checkins.findIndex(x => x.id === c.id);
    if (i >= 0) S.checkins[i] = c; else S.checkins.push(c);
    S.checkins.sort((a, b) => b.visitedAt - a.visitedAt);
    S.draft = null;
    closeSheet();
    drawCheckins();
    refresh();
    toast(d.editing ? 'Đã cập nhật check-in' : `Đã check-in ${c.name}`);
    if (!d.editing && S.view === 'map') map.panTo([c.lat, c.lng]);
  }

  /* --- chọn vị trí trên bản đồ --- */
  function startPick() {
    const d = S.draft;
    S.picking = true;
    closeSheet();
    go('map');
    map.setView([d.lat, d.lng], Math.max(map.getZoom(), 16));
    $('#pickCross').hidden = false;
    $('#pickBar').hidden = false;
    renderTrack();
  }
  function endPick(ok) {
    S.picking = false;
    $('#pickCross').hidden = true;
    $('#pickBar').hidden = true;
    renderTrack();
    const d = S.draft;
    if (!d) return;
    if (ok) {
      const c = map.getCenter();
      d.lat = c.lat; d.lng = c.lng; d.address = ''; d.source = 'map'; d.locating = false; d.dirty = true;
      reverseGeocode();
    }
    renderForm();
  }

  /* ================= Chi tiết check-in ================= */
  function openDetail(id) {
    const c = S.checkins.find(x => x.id === id);
    if (!c) return;
    map.closePopup();
    const cat = CAT[c.cat] || CAT.other;
    const j = c.journeyId && S.journeys.find(x => x.id === c.journeyId);
    const mood = MOOD[c.mood];
    openSheet(`
      ${c.photoIds.length ? `<div class="carousel">${c.photoIds.map((p, i) => `<button type="button" data-lb="${i}"><img data-photo="${p}" data-size="full" alt=""></button>`).join('')}</div>` : ''}
      <div class="detail-title">
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
      <div class="actions-row">
        <button type="button" class="btn soft" data-act="show">${icon('map')}Trên bản đồ</button>
        <a class="btn line" href="https://www.google.com/maps/dir/?api=1&destination=${c.lat},${c.lng}" target="_blank" rel="noopener">${icon('nav')}Chỉ đường</a>
      </div>
    `, `
      <button type="button" class="btn ghost" data-act="del" aria-label="Xoá">${icon('trash')}</button>
      <button type="button" class="btn primary" data-act="edit">${icon('edit')}Sửa</button>
    `, 'detail');
    $('#sheetInner').onclick = e => {
      const b = e.target.closest('button');
      if (!b) return;
      if (b.dataset.lb != null) openLightbox(c.photoIds, +b.dataset.lb, c.name);
      else if (b.dataset.journey) openJourney(b.dataset.journey);
      else if (b.dataset.act === 'show') { closeSheet(); go('map'); map.flyTo([c.lat, c.lng], 17, { duration: .8 }); }
    };
    $('#sheetFoot').onclick = async e => {
      const act = e.target.closest('[data-act]')?.dataset.act;
      if (act === 'edit') openCheckinForm({ edit: c.id });
      else if (act === 'del') {
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
    drawCheckins();
    refresh();
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
      const i = Math.round(tr.scrollLeft / tr.clientWidth);
      $('#lbCap').textContent = cap(i);
    };
  }

  /* ================= Trang Hành trình ================= */
  function routeSvg(j, w = 92, h = 92) {
    const pts = j.points;
    const cs = jCheckins(j.id);
    const all = pts.map(p => [p[0], p[1]]).concat(cs.map(c => [c.lat, c.lng]));
    let grid = '';
    for (let i = 1; i < 4; i++) grid += `<path d="M${(w / 4) * i} 0V${h}M0 ${(h / 4) * i}H${w}" stroke="#E9DCCB" stroke-width="1"/>`;
    if (all.length < 2) {
      return `<svg viewBox="0 0 ${w} ${h}">${grid}<circle cx="${w / 2}" cy="${h / 2}" r="5" fill="#E57F9D"/></svg>`;
    }
    const lat0 = all.reduce((a, p) => a + p[0], 0) / all.length;
    const k = Math.cos(lat0 * Math.PI / 180);
    const xy = all.map(p => [p[1] * k, -p[0]]);
    const minX = Math.min(...xy.map(p => p[0])), maxX = Math.max(...xy.map(p => p[0]));
    const minY = Math.min(...xy.map(p => p[1])), maxY = Math.max(...xy.map(p => p[1]));
    const span = Math.max(maxX - minX, maxY - minY) || 1e-6;
    const padd = 12, s = (w - padd * 2) / span;
    const ox = padd + ((w - padd * 2) - (maxX - minX) * s) / 2, oy = padd + ((h - padd * 2) - (maxY - minY) * s) / 2;
    const P = p => [(ox + (p[1] * k - minX) * s).toFixed(1), (oy + (-p[0] - minY) * s).toFixed(1)];
    const lines = segLines(pts).map(l => `<polyline points="${l.map(p => P(p).join(',')).join(' ')}" fill="none" stroke="#E57F9D" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"/>`).join('');
    const dots = cs.map(c => { const [x, y] = P([c.lat, c.lng]); return `<circle cx="${x}" cy="${y}" r="3.6" fill="#fff" stroke="#C2577A" stroke-width="2"/>`; }).join('');
    const st = pts.length ? P(pts[0]) : null;
    return `<svg viewBox="0 0 ${w} ${h}">${grid}${lines}${dots}${st ? `<circle cx="${st[0]}" cy="${st[1]}" r="4" fill="#C2577A"/>` : ''}</svg>`;
  }

  function renderJourneys() {
    const el = $('#journeyList');
    if (!S.journeys.length) {
      el.innerHTML = emptyState('route', 'Chưa có hành trình nào', 'Vào tab Bản đồ và bấm “Bắt đầu” để app ghi lại từng con đường bạn đi qua.', true);
      return;
    }
    let html = '', lastM = '';
    S.journeys.slice().sort((a, b) => b.startedAt - a.startedAt).forEach(j => {
      const d = new Date(j.startedAt);
      const m = `Tháng ${d.getMonth() + 1}, ${d.getFullYear()}`;
      if (m !== lastM) { html += `<p class="month-label">${m}</p>`; lastM = m; }
      const cs = jCheckins(j.id);
      const live = j.status !== 'done';
      html += `
        <button type="button" class="card jcard" data-journey="${j.id}">
          <span class="thumb">${routeSvg(j)}</span>
          <span class="meta">
            <span class="ttl">${esc(j.name)}</span>
            <span class="date">${WEEKDAYS[d.getDay()]}, ${dDate(j.startedAt)} · ${dTime(j.startedAt)}
              ${live ? ' <span class="badge">Đang ghi</span>' : ''}${j.demo ? ' <span class="badge beige">Mẫu</span>' : ''}</span>
            <span class="nums">
              <span>${icon('route')}${km(j.distance)}</span>
              <span>${icon('clock')}${dur(elapsed(j))}</span>
              <span>${icon('pin')}${cs.length}</span>
              <span>${icon('camera')}${cs.reduce((a, c) => a + c.photoIds.length, 0)}</span>
            </span>
          </span>
        </button>`;
    });
    el.innerHTML = html;
  }

  function openJourney(id) {
    const j = S.journeys.find(x => x.id === id);
    if (!j) return;
    const cs = jCheckins(id).sort((a, b) => a.visitedAt - b.visitedAt);
    const ms = elapsed(j);
    const speed = ms > 60000 ? (j.distance / 1000) / (ms / 3600000) : 0;
    const photos = cs.flatMap(c => c.photoIds);
    openSheet(`
      <div class="sheet-head">
        <h2>${esc(j.name)}</h2>
        <button type="button" class="icon-btn" data-act="close" aria-label="Đóng">${icon('x')}</button>
      </div>
      <p class="foot-note" style="text-align:left;margin-top:-10px">${WEEKDAYS[new Date(j.startedAt).getDay()]}, ${dDate(j.startedAt)} · ${dTime(j.startedAt)}${j.endedAt ? ` – ${dTime(j.endedAt)}` : ''}${j.demo ? ' · <span class="badge beige">Mẫu</span>' : ''}</p>
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
          <span class="t"><b>${esc(c.name)}</b><small>${(CAT[c.cat] || CAT.other).e} ${dTime(c.visitedAt)} · ${c.rating ? '♥'.repeat(c.rating) : 'chưa chấm'}</small></span>
          ${icon('chevron')}
        </button>`).join('')}</div>` : '<p class="foot-note" style="text-align:left">Chưa check-in điểm nào trong hành trình này.</p>'}
    `, `
      <button type="button" class="btn ghost" data-act="del" aria-label="Xoá hành trình">${icon('trash')}</button>
      <button type="button" class="btn line" data-act="rename">${icon('edit')}Sửa</button>
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
        if (!(await ask('Xoá hành trình này?', 'Lộ trình sẽ bị xoá. Các check-in và ảnh vẫn được giữ trong Kỉ niệm.'))) return;
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
        refresh();
        drawRoutes();
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
      L.polyline(lines, { color: '#C2577A', weight: 5, opacity: 1, lineCap: 'round', lineJoin: 'round' }).addTo(L_focus);
      const first = j.points[0], last = j.points[j.points.length - 1];
      L.circleMarker([first[0], first[1]], { radius: 7, color: '#fff', weight: 3, fillColor: '#6FA58A', fillOpacity: 1 }).bindTooltip('Xuất phát', { className: 'route-tip' }).addTo(L_focus);
      L.circleMarker([last[0], last[1]], { radius: 7, color: '#fff', weight: 3, fillColor: '#C2577A', fillOpacity: 1 }).bindTooltip('Kết thúc', { className: 'route-tip' }).addTo(L_focus);
    }
    $('#focusTitle').textContent = `${j.name} · ${km(j.distance)}`;
    $('#focusBanner').hidden = false;
    $('#view-map').classList.add('has-focus');
    go('map');
    if (pts.length) map.fitBounds(L.latLngBounds(pts).pad(0.25), { maxZoom: 17, paddingBottomRight: [0, 120] });
    S.follow = false;
  }
  function unfocus() {
    S.focusId = null;
    L_focus.clearLayers();
    $('#focusBanner').hidden = true;
    $('#view-map').classList.remove('has-focus');
  }

  /* ================= Trang Kỉ niệm ================= */
  function filteredCheckins() {
    const q = S.memQuery.trim().toLowerCase();
    return S.checkins.filter(c => {
      if (S.memCat === 'fav' && c.rating < 5) return false;
      if (S.memCat === 'photo' && !c.photoIds.length) return false;
      if (!['all', 'fav', 'photo'].includes(S.memCat) && c.cat !== S.memCat) return false;
      if (!q) return true;
      return [c.name, c.address, c.feedback, c.memory, ...c.tags].join(' ').toLowerCase().includes(q);
    }).sort((a, b) => b.visitedAt - a.visitedAt);
  }

  function renderMemFilters() {
    const counts = {};
    S.checkins.forEach(c => { counts[c.cat] = (counts[c.cat] || 0) + 1; });
    const opts = [{ id: 'all', label: `Tất cả · ${S.checkins.length}` }, { id: 'fav', label: '♥ Yêu thích' }, { id: 'photo', label: 'Có ảnh' }]
      .concat(CATS.filter(c => counts[c.id]).map(c => ({ id: c.id, label: `${c.e} ${c.label} · ${counts[c.id]}` })));
    $('#memFilters').innerHTML = opts.map(o => `<button type="button" class="chip ${S.memCat === o.id ? 'is-on' : ''}" data-memcat="${o.id}">${o.label}</button>`).join('');
  }

  function photoCaption(pid) {
    const c = S.checkins.find(x => x.photoIds.includes(pid));
    return c ? `${c.name} · ${dDate(c.visitedAt)}` : '';
  }

  function renderMemories() {
    renderMemFilters();
    const el = $('#memoryList');
    const list = filteredCheckins();
    if (!S.checkins.length) {
      el.innerHTML = emptyState('heart', 'Chưa có kỉ niệm nào', 'Bấm nút Check-in màu hồng ở giữa để lưu lại quán ăn, chỗ chơi, kèm ảnh và cảm nhận của bạn.', true);
      return;
    }
    if (!list.length) {
      el.innerHTML = emptyState('search', 'Không tìm thấy', 'Thử bỏ bớt bộ lọc hoặc tìm bằng từ khác.');
      return;
    }
    if (S.memMode === 'gallery') {
      const ids = list.flatMap(c => c.photoIds);
      el.innerHTML = ids.length
        ? `<div class="gallery">${ids.map((p, i) => `<button type="button" data-lb="${i}"><img data-photo="${p}" alt=""></button>`).join('')}</div><p class="foot-note">${ids.length} ảnh từ ${list.filter(c => c.photoIds.length).length} địa điểm</p>`
        : emptyState('image', 'Chưa có ảnh', 'Thêm ảnh khi check-in để album đầy dần lên.');
      el.dataset.ids = JSON.stringify(ids);
      hydratePhotos(el);
      return;
    }
    let html = '', lastDay = '';
    list.forEach(c => {
      const k = dayKey(c.visitedAt);
      if (k !== lastDay) {
        if (lastDay) html += '</div>';
        const d = new Date(c.visitedAt);
        html += `<div class="day"><div class="day-head"><span class="d">${pad(d.getDate())}.${pad(d.getMonth() + 1)}</span><span class="w">${WEEKDAYS[d.getDay()]}, ${d.getFullYear()}</span></div>`;
        lastDay = k;
      }
      const cat = CAT[c.cat] || CAT.other;
      const ph = c.photoIds.slice(0, 3);
      const more = c.photoIds.length - 3;
      html += `
        <button type="button" class="card mcard" data-ci="${c.id}">
          ${ph.length ? `<span class="photos n${ph.length}">${ph.map((p, i) => `<span class="${i === 2 && more > 0 ? 'more' : ''}" data-more="+${more}"><img data-photo="${p}" alt=""></span>`).join('')}</span>` : ''}
          <span class="body">
            <span class="top">
              <span class="cat">${cat.e}</span>
              <span class="t"><span class="ttl">${esc(c.name)} ${c.demo ? '<span class="badge beige">Mẫu</span>' : ''}</span><span class="sub">${dTime(c.visitedAt)} · ${cat.label}${c.price != null ? ' · ' + PRICES[c.price] : ''}${c.address ? ' · ' + esc(c.address) : ''}</span></span>
              ${c.mood ? `<span class="mood" title="${MOOD[c.mood].l}">${MOOD[c.mood].e}</span>` : ''}
            </span>
            ${c.rating ? hearts(c.rating) : ''}
            ${c.memory ? `<p class="memo">${esc(c.memory)}</p>` : ''}
            ${c.feedback ? `<p class="fb">${esc(c.feedback)}</p>` : ''}
            ${c.tags.length ? `<span class="tags">${c.tags.slice(0, 4).map(t => `<span class="tag">${esc(t)}</span>`).join('')}</span>` : ''}
          </span>
        </button>`;
    });
    html += '</div>';
    el.innerHTML = html;
    hydratePhotos(el);
  }

  /* ================= Trang Thống kê ================= */
  function renderStats() {
    const el = $('#statsBody');
    const J = S.journeys, C = S.checkins;
    if (!J.length && !C.length) {
      el.innerHTML = emptyState('chart', 'Chưa có gì để thống kê', 'Khi bạn ghi hành trình và check-in, số km, địa điểm yêu thích và tâm trạng sẽ hiện ở đây.', true) + dataPanel();
      hydrateIcons(el);
      return;
    }
    const dist = J.reduce((a, j) => a + j.distance, 0);
    const moving = J.reduce((a, j) => a + elapsed(j), 0);
    const photos = C.reduce((a, c) => a + c.photoIds.length, 0);
    const places = new Set(C.map(c => c.name.trim().toLowerCase())).size;
    const days = new Set(C.map(c => dayKey(c.visitedAt)).concat(J.map(j => dayKey(j.startedAt)))).size;
    const steps = Math.round(dist / STRIDE_M);

    // theo loại
    const byCat = CATS.map(c => ({ ...c, n: C.filter(x => x.cat === c.id).length })).filter(c => c.n).sort((a, b) => b.n - a.n);
    const maxCat = Math.max(1, ...byCat.map(c => c.n));

    // 6 tháng gần nhất
    const now = new Date();
    const months = [];
    for (let i = 5; i >= 0; i--) {
      const d = new Date(now.getFullYear(), now.getMonth() - i, 1);
      months.push({ y: d.getFullYear(), m: d.getMonth(), n: 0, km: 0 });
    }
    C.forEach(c => { const d = new Date(c.visitedAt); const mm = months.find(x => x.y === d.getFullYear() && x.m === d.getMonth()); if (mm) mm.n++; });
    J.forEach(j => { const d = new Date(j.startedAt); const mm = months.find(x => x.y === d.getFullYear() && x.m === d.getMonth()); if (mm) mm.km += j.distance / 1000; });
    const maxM = Math.max(1, ...months.map(m => m.n));

    // đánh giá
    const rated = C.filter(c => c.rating);
    const avg = rated.length ? rated.reduce((a, c) => a + c.rating, 0) / rated.length : 0;
    const dist5 = [5, 4, 3, 2, 1].map(r => ({ r, n: rated.filter(c => c.rating === r).length }));
    const maxR = Math.max(1, ...dist5.map(d => d.n));

    // top
    const top = C.filter(c => c.rating).sort((a, b) => b.rating - a.rating || b.photoIds.length - a.photoIds.length || b.visitedAt - a.visitedAt).slice(0, 5);
    const longest = J.filter(j => j.status === 'done').sort((a, b) => b.distance - a.distance)[0];
    const moodN = MOODS.map(m => ({ ...m, n: C.filter(c => c.mood === m.v).length }));
    const favCat = byCat[0];
    const backAgain = C.filter(c => c.tags.includes('Đáng quay lại')).length;

    el.innerHTML = `
      <div class="hero-stat">
        <span class="steps">${icon('shoe')}</span>
        <span class="lbl">Tổng quãng đường đã ghi</span>
        <span class="big">${kmNum(dist)}<small>km</small></span>
        <span class="note">≈ ${nf.format(steps)} bước chân của bạn</span>
      </div>
      <div class="tiles">
        <div class="card tile"><b>${J.length}</b><span>Hành trình</span></div>
        <div class="card tile"><b>${C.length}</b><span>Lượt check-in</span></div>
        <div class="card tile"><b>${places}</b><span>Địa điểm</span></div>
        <div class="card tile"><b>${photos}</b><span>Ảnh kỉ niệm</span></div>
        <div class="card tile"><b>${days}</b><span>Ngày khám phá</span></div>
        <div class="card tile"><b>${moving >= 3600000 ? nf1.format(moving / 3600000) : Math.round(moving / 60000)}</b><span>${moving >= 3600000 ? 'Giờ di chuyển' : 'Phút di chuyển'}</span></div>
      </div>

      ${byCat.length ? `<section class="card panel">
        <h2>Bạn hay đi đâu nhất? <small>${favCat ? `${favCat.e} ${favCat.label}` : ''}</small></h2>
        <div class="bars">${byCat.map(c => `<div class="bar-row"><span class="name">${c.e} ${c.label}</span><span class="track"><span class="fill" style="width:${(c.n / maxCat) * 100}%"></span></span><span class="n">${c.n}</span></div>`).join('')}</div>
      </section>` : ''}

      <section class="card panel">
        <h2>Check-in theo tháng <small>6 tháng gần đây</small></h2>
        <div class="cols">${months.map(m => `<div class="col"><div class="stack" style="height:100%"><span class="v ${m.n ? '' : 'zero'}" style="height:${m.n ? Math.max(6, (m.n / maxM) * 100) : 2}%"></span>${m.n ? `<span class="num" style="bottom:${Math.max(6, (m.n / maxM) * 100)}%">${m.n}</span>` : ''}</div><span class="m">T${m.m + 1}</span></div>`).join('')}</div>
        <p class="foot-note" style="text-align:left;margin:0">Quãng đường tháng này: <b>${nf1.format(months[5].km)} km</b> · tháng trước: ${nf1.format(months[4].km)} km</p>
      </section>

      ${rated.length ? `<section class="card panel">
        <h2>Feedback của bạn <small>${rated.length} nơi đã chấm</small></h2>
        <div class="rate-row"><span class="rate-big">${nf1.format(avg)}<small> /5</small></span>${hearts(Math.round(avg))}</div>
        <div class="bars">${dist5.map(d => `<div class="bar-row"><span class="name">${'♥'.repeat(d.r)}</span><span class="track"><span class="fill" style="width:${(d.n / maxR) * 100}%"></span></span><span class="n">${d.n}</span></div>`).join('')}</div>
        ${backAgain ? `<p class="foot-note" style="text-align:left;margin:0">${backAgain} nơi bạn đánh dấu “Đáng quay lại”.</p>` : ''}
      </section>` : ''}

      ${top.length ? `<section class="card panel">
        <h2>Nơi bạn thương nhất</h2>
        <div class="top-list">${top.map((c, i) => `
          <button type="button" class="top-item" data-ci="${c.id}">
            <span class="rank">${i + 1}</span>
            ${c.photoIds[0] ? `<img data-photo="${c.photoIds[0]}" alt="">` : `<span class="ph">${(CAT[c.cat] || CAT.other).e}</span>`}
            <span class="t"><b>${esc(c.name)}</b><small>${'♥'.repeat(c.rating)} · ${dDate(c.visitedAt)}</small></span>
          </button>`).join('')}</div>
      </section>` : ''}

      ${longest ? `<section class="card panel">
        <h2>Hành trình dài nhất</h2>
        <button type="button" class="card jcard" data-journey="${longest.id}" style="box-shadow:none;background:var(--blush);border:0">
          <span class="thumb">${routeSvg(longest)}</span>
          <span class="meta"><span class="ttl">${esc(longest.name)}</span><span class="date">${dDate(longest.startedAt)}</span>
          <span class="nums"><span>${icon('route')}${km(longest.distance)}</span><span>${icon('clock')}${dur(longest.activeMs)}</span></span></span>
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
        <p class="foot-note" style="text-align:left;margin:0">Mọi thứ được lưu ngay trên điện thoại này, không gửi lên máy chủ nào. Hãy sao lưu định kỳ để không mất kỉ niệm.</p>
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
      app: 'dau-chan-hong', version: 1, exportedAt: new Date().toISOString(),
      journeys: await DB.all('journeys'), checkins: await DB.all('checkins'),
      photos: await Promise.all(photos.map(async p => ({ id: p.id, checkinId: p.checkinId, createdAt: p.createdAt, demo: p.demo, blob: await blobToData(p.blob), thumb: await blobToData(p.thumb) })))
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
      for (const p of data.photos || []) {
        await DB.put('photos', { ...p, blob: await dataToBlob(p.blob), thumb: await dataToBlob(p.thumb) });
      }
      for (const c of data.checkins || []) await DB.put('checkins', c);
      for (const j of data.journeys || []) await DB.put('journeys', j.status === 'active' ? { ...j, status: 'paused', resumedAt: null } : j);
      await loadAll();
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
    fitAll();
    toast('Đã thêm chuyến đi mẫu ở Đà Lạt & Hà Nội');
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

  /* ================= Tìm kiếm ================= */
  async function search(q) {
    const box = $('#searchResults');
    q = q.trim();
    if (!q) { box.hidden = true; return; }
    const local = S.checkins.filter(c => c.name.toLowerCase().includes(q.toLowerCase())).slice(0, 4);
    box.hidden = false;
    box.innerHTML = renderLocal(local) + '<li class="empty">Đang tìm trên bản đồ…</li>';
    let remote = [];
    try {
      const c = map.getCenter();
      const res = await fetch(`https://nominatim.openstreetmap.org/search?format=jsonv2&q=${encodeURIComponent(q)}&accept-language=vi&limit=8&viewbox=${c.lng - 0.5},${c.lat + 0.5},${c.lng + 0.5},${c.lat - 0.5}&bounded=0`);
      remote = res.ok ? await res.json() : [];
    } catch { remote = null; }
    box.innerHTML = renderLocal(local) + (remote === null
      ? '<li class="empty">Không kết nối được dịch vụ tìm kiếm.</li>'
      : remote.length
        ? remote.map(r => `<li><button type="button" data-lat="${r.lat}" data-lng="${r.lon}" data-name="${esc(r.name || r.display_name.split(',')[0])}" data-addr="${esc(r.display_name)}"><b>${esc(r.name || r.display_name.split(',')[0])}</b><small>${esc(r.display_name)}</small></button></li>`).join('')
        : (local.length ? '' : '<li class="empty">Không tìm thấy địa điểm phù hợp.</li>'));
  }
  const renderLocal = list => list.map(c => `<li><button type="button" data-ci="${c.id}"><b>${(CAT[c.cat] || CAT.other).e} ${esc(c.name)}</b><small>Đã check-in · ${dDate(c.visitedAt)}</small></button></li>`).join('');

  function showSearchResult(lat, lng, name, addr) {
    $('#searchResults').hidden = true;
    if (tempMarker) map.removeLayer(tempMarker);
    const el = document.createElement('div');
    el.className = 'pop';
    el.innerHTML = `<b>${esc(name)}</b><small>${esc(addr.split(',').slice(1, 4).join(','))}</small><br><button type="button">＋ Check-in tại đây</button>`;
    el.querySelector('button').onclick = () => {
      map.removeLayer(tempMarker); tempMarker = null;
      openCheckinForm({ lat, lng, name, address: addr.split(',').slice(0, 3).join(','), fromMap: true });
    };
    tempMarker = L.marker([lat, lng], { icon: L.divIcon({ className: 'pin-wrap', html: '<div class="pin"><span>🔎</span></div>', iconSize: [38, 38], iconAnchor: [19, 46], popupAnchor: [0, -40] }) })
      .addTo(map).bindPopup(el, { closeButton: false });
    S.follow = false;
    map.flyTo([lat, lng], 17, { duration: .8 });
    setTimeout(() => tempMarker && tempMarker.openPopup(), 850);
  }

  /* ================= Điều hướng ================= */
  function go(view) {
    S.view = view;
    $$('.view').forEach(v => v.classList.toggle('is-active', v.dataset.view === view));
    $$('.tab[data-go]').forEach(t => t.classList.toggle('is-on', t.dataset.go === view));
    if (view === 'map') setTimeout(() => map.invalidateSize(), 0);
    renderTrack();
    renderView();
  }
  function renderView() {
    if (S.view === 'journeys') renderJourneys();
    else if (S.view === 'memories') renderMemories();
    else if (S.view === 'stats') renderStats();
    hydrateIcons($(`#view-${S.view}`));
  }
  function refresh() { renderTrack(); renderView(); }

  async function loadAll() {
    S.journeys = (await DB.all('journeys')).sort((a, b) => b.startedAt - a.startedAt);
    S.checkins = (await DB.all('checkins')).sort((a, b) => b.visitedAt - a.visitedAt);
    S.checkins.forEach(c => { c.tags = c.tags || []; c.photoIds = c.photoIds || []; });
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
      toast('Hành trình đang dở. Bấm Tiếp tục để ghi tiếp.');
    } else if (S.active) {
      S.active = S.journeys.find(j => j.id === S.active.id) || null;
    }
    drawCheckins();
    drawRoutes();
    drawActive();
    refresh();
  }

  function onboarding() {
    DB.setMeta('onboarded', true).catch(() => {});
    openSheet(`
      <div class="onboard">
        <img class="logo" src="icons/icon.svg" alt="">
        <h2>Dấu Chân Hồng</h2>
        <p class="script">mỗi con đường đều đáng nhớ</p>
        <ul>
          <li><span class="ic">${icon('route')}</span><span>Ghi lại lộ trình bạn đi qua trên bản đồ thật bằng GPS</span></li>
          <li><span class="ic">${icon('pin')}</span><span>Check-in quán ăn, quán cà phê, chỗ chơi kèm ảnh và feedback</span></li>
          <li><span class="ic">${icon('heart')}</span><span>Lưu kỉ niệm, tâm trạng và xem lại theo dòng thời gian</span></li>
          <li><span class="ic">${icon('chart')}</span><span>Thống kê số km, nơi yêu thích, tâm trạng mỗi chuyến đi</span></li>
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
      else locateOnce(true);
    };
  }

  /* ================= Gắn sự kiện ================= */
  function bind() {
    $$('.tab[data-go]').forEach(t => t.onclick = () => go(t.dataset.go));
    $('#btnCheckin').onclick = () => openCheckinForm();
    $('#scrim').onclick = scrimClose;
    window.addEventListener('popstate', () => { if (S.sheetOpen) { if (S.sheetKind === 'form') S.draft = null; closeSheet(true); } S.sheetHist = false; });

    $('#trackCard').onclick = e => {
      const act = e.target.closest('[data-act]')?.dataset.act;
      if (act === 'start') startJourney();
      else if (act === 'pause') pauseJourney();
      else if (act === 'resume') resumeJourney();
      else if (act === 'end') ask('Kết thúc hành trình?', 'Lộ trình sẽ được lưu lại, bạn có thể đặt tên và ghi cảm nhận.', 'Kết thúc', false).then(ok => ok && endJourney());
    };

    $('#btnLocate').onclick = () => { S.follow = true; $('#btnLocate').classList.add('is-on'); locateOnce(true); };
    $('#btnLayers').onclick = () => {
      const keys = Object.keys(TILES);
      const next = keys[(keys.indexOf(S.tile) + 1) % keys.length];
      setTile(next);
      DB.setMeta('tile', next);
      toast(TILES[next].name);
    };
    $('#btnRoutes').onclick = () => {
      S.showRoutes = !S.showRoutes;
      $('#btnRoutes').classList.toggle('is-on', S.showRoutes);
      drawRoutes();
      toast(S.showRoutes ? 'Hiện mọi lộ trình đã đi' : 'Ẩn lộ trình cũ');
    };
    $('#focusClose').onclick = unfocus;
    $('#pickOk').onclick = () => endPick(true);
    $('#pickCancel').onclick = () => endPick(false);

    const si = $('#searchInput');
    $('#searchForm').onsubmit = e => { e.preventDefault(); si.blur(); search(si.value); };
    si.oninput = () => { $('#searchClear').hidden = !si.value; if (!si.value) $('#searchResults').hidden = true; };
    $('#searchClear').onclick = () => { si.value = ''; $('#searchClear').hidden = true; $('#searchResults').hidden = true; if (tempMarker) { map.removeLayer(tempMarker); tempMarker = null; } };
    $('#searchResults').onclick = e => {
      const b = e.target.closest('button');
      if (!b) return;
      if (b.dataset.ci) { $('#searchResults').hidden = true; const c = S.checkins.find(x => x.id === b.dataset.ci); map.flyTo([c.lat, c.lng], 17); openDetail(c.id); }
      else showSearchResult(+b.dataset.lat, +b.dataset.lng, b.dataset.name, b.dataset.addr);
    };

    // Trang
    $('#journeyList').onclick = e => { const b = e.target.closest('[data-journey]'); if (b) openJourney(b.dataset.journey); };
    $('#memoryList').onclick = e => {
      const b = e.target.closest('[data-ci],[data-lb]');
      if (!b) return;
      if (b.dataset.ci) openDetail(b.dataset.ci);
      else { const ids = JSON.parse($('#memoryList').dataset.ids || '[]'); openLightbox(ids, +b.dataset.lb, i => photoCaption(ids[i])); }
    };
    $('#memFilters').onclick = e => { const b = e.target.closest('[data-memcat]'); if (b) { S.memCat = b.dataset.memcat; renderMemories(); } };
    $('#memSearch').oninput = e => { S.memQuery = e.target.value; renderMemories(); };
    $$('[data-memmode]').forEach(b => b.onclick = () => {
      S.memMode = b.dataset.memmode;
      $$('[data-memmode]').forEach(x => x.classList.toggle('is-on', x === b));
      renderMemories();
    });
    $('#statsBody').onclick = e => {
      const b = e.target.closest('[data-ci],[data-journey],[data-act]');
      if (!b) return;
      if (b.dataset.ci) openDetail(b.dataset.ci);
      else if (b.dataset.journey) openJourney(b.dataset.journey);
    };
    document.addEventListener('click', e => {
      const b = e.target.closest('[data-act]');
      if (!b || b.closest('#sheet') || b.closest('#trackCard')) return;
      const act = b.dataset.act;
      if (act === 'demo') addDemo();
      else if (act === 'cleardemo') clearDemo();
      else if (act === 'export') exportData();
      else if (act === 'import') $('#fileImport').click();
      else if (act === 'wipe') wipeAll();
    });
    $('#fileImport').onchange = e => { const f = e.target.files[0]; if (f) importData(f); e.target.value = ''; };

    $('#lbClose').onclick = () => { $('#lightbox').hidden = true; };
    document.addEventListener('keydown', e => {
      if (e.key !== 'Escape') return;
      if (!$('#lightbox').hidden) $('#lightbox').hidden = true;
      else if (S.sheetOpen) scrimClose();
    });

    setInterval(() => { if (S.active?.status === 'active' && S.view === 'map') renderTrack(); }, 1000);
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible' && S.active?.status === 'active') wake(true);
      if (document.visibilityState === 'hidden') saveActive();
    });
    window.addEventListener('pagehide', saveActive);
  }

  /* ================= Khởi động ================= */
  async function init() {
    hydrateIcons();
    initMap();
    bind();
    try {
      await DB.open();
      S.tile = await DB.getMeta('tile', 'voyager');
      setTile(S.tile);
      await loadAll();
    } catch (e) {
      toast('Trình duyệt chặn bộ nhớ cục bộ, dữ liệu sẽ không được lưu.');
      refresh();
    }
    if (S.checkins.length || S.journeys.length) fitAll();
    let onboarded = false;
    try { onboarded = await DB.getMeta('onboarded', false); } catch { /* bỏ qua */ }
    if (!onboarded) onboarding();
    else if (navigator.permissions) {
      try {
        const p = await navigator.permissions.query({ name: 'geolocation' });
        if (p.state === 'granted') locateOnce(!S.checkins.length);
      } catch { /* bỏ qua */ }
    }
    if ('serviceWorker' in navigator && location.protocol !== 'file:') {
      navigator.serviceWorker.register('sw.js').catch(() => {});
    }
  }

  init();
})();
