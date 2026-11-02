// SonVu Raw Pick UI. Talks to the Rust side through Tauri commands; all heavy work happens there.
"use strict";

const T = window.__TAURI__;
const invoke = (cmd, args) => T.core.invoke(cmd, args);
const fileSrc = (p) => (p ? T.core.convertFileSrc(p) : "");
const $ = (id) => document.getElementById(id);

const CRIT_LABEL = { exposure: "Phơi sáng", sharp: "Độ nét", comp: "Bố cục", color: "Màu sắc", noise: "Nhiễu", people: "Người" };
const PRESETS = {
  balanced: [20, 25, 15, 10, 10, 20],
  event: [20, 25, 10, 10, 5, 30],
  landscape: [25, 15, 30, 20, 10, 5],
  portrait: [20, 20, 10, 10, 5, 35],
};
const REASONS = {
  "closed-eyes": "Nhắm mắt", blurry: "Mờ, rung, trượt nét", overexposed: "Cháy sáng / thừa sáng", underexposed: "Thiếu sáng",
  tilted: "Nghiêng", composition: "Sai bố cục", noise: "Nhiễu hạt nặng", duplicate: "Trùng lặp", broken: "File hỏng", "low-score": "Điểm thấp",
};
const CORE_OPTS = [
  ["reject_closed_eyes", "Loại ảnh có người nhắm mắt"],
  ["dedupe", "Lọc ảnh trùng, giữ ảnh đẹp nhất mỗi loạt"],
  ["use_learned", "Dùng điều AI đã học từ lựa chọn của bạn"],
];

const st = {
  info: null, prefs: null, session: null, verdicts: [], summary: null,
  tab: "all", reason: "", sort: "time", view: [], cur: -1,
};

// ---------------------------------------------------------------- helpers

function toast(html, ms = 5000) {
  const t = $("toast");
  t.innerHTML = html;
  t.hidden = false;
  clearTimeout(toast.h);
  toast.h = setTimeout(() => (t.hidden = true), ms);
}
function esc(s) {
  return String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
}
function cls(v) { return v >= 7.5 ? "hi" : v >= 5 ? "mid" : "lo"; }
function decKind(d) { return d.kind; }
function decLabel(d) {
  if (d.kind === "selected") return "Đã chọn";
  if (d.kind === "review") return "Cần xem lại";
  return "Bị loại: " + (REASONS[d.reason] || d.reason);
}
function fmtTime(t) {
  if (t == null) return "";
  const d = new Date((t + 946684800) * 1000);
  return d.toISOString().slice(11, 19);
}

let busyOn = false;
function busy(on, text) {
  busyOn = on;
  $("busy").hidden = !on;
  if (text) $("busyText").textContent = text;
  $("busyBar").style.width = "0%";
  $("busyCount").textContent = "";
}
T.event.listen("progress", (e) => {
  if (!busyOn) return;
  const { done, total, stage } = e.payload;
  $("busyText").textContent = stage + "…";
  $("busyBar").style.width = total ? (100 * done) / total + "%" : "0%";
  $("busyCount").textContent = `${done} / ${total}`;
});
$("btnCancel").onclick = () => invoke("cancel");

// ---------------------------------------------------------------- settings rail

function renderMeta() {
  const i = st.info;
  $("meta").textContent = `v${i.version} · ${i.threads} luồng CPU · AI khuôn mặt: ${i.face_ok ? "bật" : "chưa có"}`;
  if (i.face_warning) {
    $("faceWarn").hidden = false;
    $("faceWarn").textContent = i.face_warning;
  }
}

function renderPresets() {
  for (const b of $("presets").children) b.setAttribute("aria-pressed", String(b.dataset.p === st.prefs.preset));
}

function renderWeights() {
  const w = st.prefs.settings.weights;
  $("weights").innerHTML = st.info.criteria
    .map((k, i) => `<div class="wrow"><span>${CRIT_LABEL[k]}</span><input type="range" min="0" max="50" step="5" value="${w[i]}" data-i="${i}" aria-label="Trọng số ${CRIT_LABEL[k]}"><output>${w[i]}</output></div>`)
    .join("");
  for (const r of $("weights").querySelectorAll("input")) {
    r.oninput = () => {
      r.nextElementSibling.textContent = r.value;
    };
    r.onchange = () => {
      st.prefs.settings.weights[+r.dataset.i] = +r.value;
      st.prefs.preset = "custom";
      renderPresets();
      savePrefs();
    };
  }
}

function renderRules() {
  const s = st.prefs.settings;
  $("thr").value = s.threshold;
  $("thrOut").textContent = s.threshold.toFixed(1);
  $("mar").value = s.review_margin;
  $("marOut").textContent = s.review_margin.toFixed(1);
  $("maxSel").value = s.max_selected ?? "";
  $("dupSec").value = s.dedupe_seconds;
  $("keepSeries").value = s.keep_per_series;
  $("optRecursive").checked = st.prefs.recursive;
  $("optAuto").checked = s.options.auto_apply;
  $("expReview").checked = st.prefs.export.copy_review;
  $("expRejected").checked = st.prefs.export.copy_rejected;
}

function suggestionFor(key) {
  return (st.session?.suggestions || []).find((s) => s.key === key);
}

function renderOptions() {
  const o = st.prefs.settings.options;
  $("coreOpts").innerHTML = CORE_OPTS.map(([k, t]) => `<label class="chk"><input type="checkbox" data-k="${k}" ${o[k] ? "checked" : ""}> ${t}</label>`).join("");
  $("featOpts").innerHTML = st.info.catalog
    .map((f) => {
      const soon = f.phase > 1;
      const sug = suggestionFor(f.key);
      return `<div class="feat ${soon ? "soon" : ""}">
        <label class="chk t"><span><input type="checkbox" data-k="${f.key}" ${o[f.key] ? "checked" : ""}> ${esc(f.title)}</span>${soon ? `<span class="badge">giai đoạn ${f.phase}</span>` : ""}</label>
        <p class="d">${esc(f.desc)}</p>
        ${sug && sug.on ? `<p class="sug">AI gợi ý bật: ${esc(sug.reason)}</p>` : ""}
      </div>`;
    })
    .join("");
  for (const c of document.querySelectorAll("#coreOpts input, #featOpts input")) {
    c.onchange = () => {
      o[c.dataset.k] = c.checked;
      const f = st.info.catalog.find((x) => x.key === c.dataset.k);
      if (f && f.phase > 1 && c.checked) toast(`«${esc(f.title)}» sẽ có ở giai đoạn ${f.phase}. Lựa chọn của bạn được lưu lại.`);
      if (c.dataset.k === "safe_copy" && c.checked) toast("Dùng nút «Chép từ thẻ nhớ…» để chép an toàn trước khi lọc.");
      renderCams();
      savePrefs();
    };
  }
  renderCams();
}

function renderCams() {
  const box = $("cams");
  const o = st.prefs.settings.options;
  if (!o.multi_camera || !st.session) {
    box.innerHTML = "";
    return;
  }
  const cams = [...new Set(st.session.photos.map((p) => p.camera).filter((c) => c && c !== "Không rõ máy"))];
  if (cams.length < 2) {
    box.innerHTML = `<p class="note">Chỉ thấy một máy ảnh trong thư mục này.</p>`;
    return;
  }
  const off = st.prefs.settings.camera_offsets;
  box.innerHTML =
    `<p class="note">Lệch giờ từng máy (giây, cộng vào giờ của máy đó):</p>` +
    cams.map((c) => `<div class="cam"><span>${esc(c)}</span><input type="number" step="1" data-c="${esc(c)}" value="${off[c] ?? 0}"></div>`).join("");
  for (const i of box.querySelectorAll("input")) {
    i.onchange = () => {
      off[i.dataset.c] = +i.value || 0;
      savePrefs();
    };
  }
}

function renderProfile(p) {
  if (p) st.info.profile = p;
  p = st.info.profile;
  const sel = $("profileSel");
  if (!st.info.profiles.includes(st.prefs.profile)) st.info.profiles.unshift(st.prefs.profile);
  sel.innerHTML = st.info.profiles.map((n) => `<option ${n === st.prefs.profile ? "selected" : ""}>${esc(n)}</option>`).join("");
  const pct = (v) => (v == null ? "—" : Math.round(v * 100) + "%");
  $("profileBox").innerHTML = p.ready
    ? `Đã học từ <b>${p.samples}</b> ảnh · AI đồng ý với bạn <b>${pct(p.agreement)}</b> · tái hiện đúng <b>${pct(p.accuracy)}</b> lựa chọn<ul>${p.explain.map((e) => `<li>${esc(e)}</li>`).join("")}</ul>`
    : `Chưa đủ dữ liệu: <b>${p.samples}</b> / ${p.min_samples} ảnh. Mỗi lần xuất thư mục, AI học từ những ảnh bạn giữ hoặc bỏ.`;
  $("btnTrain").disabled = !st.session;
}

let saveTimer = 0;
function savePrefs() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(async () => {
    const v = await invoke("save_prefs", { prefs: st.prefs });
    if (v) applyVerdicts(v);
  }, 120);
}

function wireRail() {
  for (const b of $("presets").children) {
    b.onclick = () => {
      st.prefs.preset = b.dataset.p;
      st.prefs.settings.weights = [...PRESETS[b.dataset.p]];
      renderPresets();
      renderWeights();
      savePrefs();
    };
  }
  $("thr").oninput = () => ($("thrOut").textContent = (+$("thr").value).toFixed(1));
  $("thr").onchange = () => { st.prefs.settings.threshold = +$("thr").value; savePrefs(); };
  $("mar").oninput = () => ($("marOut").textContent = (+$("mar").value).toFixed(1));
  $("mar").onchange = () => { st.prefs.settings.review_margin = +$("mar").value; savePrefs(); };
  $("maxSel").onchange = () => { const v = parseInt($("maxSel").value, 10); st.prefs.settings.max_selected = v > 0 ? v : null; savePrefs(); };
  $("dupSec").onchange = () => { st.prefs.settings.dedupe_seconds = Math.max(0, +$("dupSec").value || 0); savePrefs(); };
  $("keepSeries").onchange = () => { st.prefs.settings.keep_per_series = Math.max(1, parseInt($("keepSeries").value, 10) || 1); savePrefs(); };
  $("optRecursive").onchange = () => { st.prefs.recursive = $("optRecursive").checked; savePrefs(); };
  $("optAuto").onchange = () => { st.prefs.settings.options.auto_apply = $("optAuto").checked; savePrefs(); };
  $("expReview").onchange = () => { st.prefs.export.copy_review = $("expReview").checked; savePrefs(); };
  $("expRejected").onchange = () => { st.prefs.export.copy_rejected = $("expRejected").checked; savePrefs(); };
  $("profileSel").onchange = async () => {
    st.prefs.profile = $("profileSel").value;
    renderProfile(await invoke("profile_info", { name: st.prefs.profile }));
    savePrefs();
  };
  $("btnNewProfile").onclick = () => {
    const n = prompt("Tên loại sự kiện mới (ví dụ: Cưới, Hội nghị, Sân khấu):");
    if (!n || !n.trim()) return;
    st.prefs.profile = n.trim();
    st.info.profiles.unshift(st.prefs.profile);
    invoke("profile_info", { name: st.prefs.profile }).then(renderProfile);
    savePrefs();
  };
  $("btnTrain").onclick = async () => {
    try {
      const p = await invoke("train");
      renderProfile(p);
      toast(p.ready ? `AI đã học xong, tái hiện đúng ${Math.round(p.accuracy * 100)}% lựa chọn của bạn.` : `Đã lưu ${p.samples} lựa chọn. Cần ${p.min_samples} ảnh có cả giữ và bỏ để AI bắt đầu học.`);
    } catch (e) { toast(esc(e)); }
  };
}

// ---------------------------------------------------------------- folder in

async function openFolder(path) {
  if (!path) return;
  busy(true, "Đang đọc thư mục…");
  try {
    const s = await invoke("analyze", { source: path });
    st.session = s;
    st.prefs = s.prefs;
    st.cur = -1;
    applyVerdicts(s);
    renderRules();
    renderOptions();
    renderProfile(s.profile);
    $("empty").hidden = true;
    $("results").hidden = false;
    if (s.applied.length) toast("AI đã tự bật: " + s.applied.map(esc).join(", "));
  } catch (e) {
    toast(esc(e));
  } finally {
    busy(false);
  }
}

async function pickFolder(title) {
  return await T.dialog.open({ directory: true, multiple: false, title });
}

function wireIn() {
  const drop = $("drop");
  drop.onclick = async () => openFolder(await pickFolder("Chọn thư mục ảnh"));
  drop.onkeydown = (e) => { if (e.key === "Enter" || e.key === " ") drop.click(); };
  T.webview.getCurrentWebview().onDragDropEvent((e) => {
    const t = e.payload.type;
    if (t === "over" || t === "enter") drop.classList.add("over");
    else drop.classList.remove("over");
    if (t === "drop" && e.payload.paths?.length) openFolder(e.payload.paths[0]);
  });

  $("btnCard").onclick = () => { $("cardDlg").hidden = false; };
  $("cardCancel").onclick = () => { $("cardDlg").hidden = true; };
  for (const b of document.querySelectorAll("[data-pick]")) {
    b.onclick = async () => {
      const p = await pickFolder(b.previousElementSibling.parentElement.parentElement.firstChild.textContent.trim());
      if (p) $(b.dataset.pick).value = p;
    };
  }
  $("cardGo").onclick = async () => {
    const card = $("cardSrc").value, dest = $("cardDst").value, backup = $("cardBak").value || null;
    if (!card || !dest) return toast("Chọn thẻ nhớ và thư mục đích trước đã.");
    $("cardDlg").hidden = true;
    busy(true, "Đang chép thẻ…");
    try {
      const r = await invoke("copy_card", { card, dest, backup });
      busy(false);
      const gb = (r.bytes / 1e9).toFixed(2);
      toast(`Đã chép ${r.copied} file (${gb} GB), bỏ qua ${r.skipped_same} file đã có` + (r.errors.length ? `, <b>${r.errors.length} lỗi</b>: ${esc(r.errors[0])}` : ""), 9000);
      if (!r.errors.length && $("cardThen").checked) openFolder(dest);
    } catch (e) {
      busy(false);
      toast(esc(e));
    }
  };
}

// ---------------------------------------------------------------- results

function applyVerdicts(v) {
  st.verdicts = v.verdicts;
  st.summary = v.summary;
  renderSummary();
  renderGrid();
  if (st.cur >= 0) renderDetail();
}

function renderSummary() {
  const s = st.summary, t = st.session.timing;
  const secs = t.seconds < 60 ? `${t.seconds.toFixed(1)} giây` : `${(t.seconds / 60).toFixed(1)} phút`;
  $("summary").innerHTML = `
    <div class="stat pick"><b>${s.selected}</b><span>đã chọn</span></div>
    <div class="stat"><b>${s.review}</b><span>cần xem lại</span></div>
    <div class="stat"><b>${s.rejected}</b><span>bị loại</span></div>
    <div class="stat"><b>${s.total}</b><span>ảnh${s.moments > 1 ? ` · ${s.moments} khoảnh khắc` : ""}</span></div>
    <p class="meta">${esc(st.session.source)} · phân tích trong ${secs} (${Math.round(t.per_photo_ms)} ms/ảnh${t.from_cache ? `, ${t.from_cache} ảnh lấy lại từ lần trước` : ""})</p>`;
  const sel = $("reasonSel");
  const cur = sel.value;
  sel.innerHTML = `<option value="">Mọi lý do</option>` + s.by_reason.map(([r, n]) => `<option value="${esc(r)}">${esc(r)} (${n})</option>`).join("");
  sel.value = cur;
  const sug = (st.session.suggestions || []).filter((x) => x.on && !st.prefs.settings.options[x.key] && x.key !== "report");
  $("suggest").hidden = !sug.length;
  $("suggest").innerHTML = `<b>AI gợi ý cho thư mục này</b> (bật ở mục Tuỳ chọn bên trái):<ul>${sug
    .map((x) => `<li>${esc(st.info.catalog.find((f) => f.key === x.key)?.title || x.key)}: ${esc(x.reason)}</li>`)
    .join("")}</ul>`;
}

function topWhy(p, v) {
  if (p.error) return p.error;
  if (v.decision.kind === "rejected") return v.notes.find((n) => !n.startsWith("Điểm bị giới hạn")) || REASONS[v.decision.reason];
  if (v.fixes.length) return "Sửa: " + v.fixes[0];
  if (v.decision.kind === "review") return v.notes[0] || "Gần ngưỡng chọn";
  return v.notes[0] || (p.camera ? p.camera : "");
}

function computeView() {
  const P = st.session.photos;
  let idx = st.verdicts.map((v) => v.idx);
  if (st.tab !== "all") idx = idx.filter((i) => st.verdicts[i].decision.kind === st.tab);
  if (st.reason) idx = idx.filter((i) => decLabel(st.verdicts[i].decision).endsWith(st.reason));
  if (st.sort === "score") idx.sort((a, b) => st.verdicts[b].score - st.verdicts[a].score);
  else if (st.sort === "name") idx.sort((a, b) => P[a].name.localeCompare(P[b].name));
  else idx.sort((a, b) => (st.verdicts[a].time ?? 1e12) - (st.verdicts[b].time ?? 1e12) || a - b);
  st.view = idx;
}

function renderGrid() {
  computeView();
  const P = st.session.photos;
  $("grid").innerHTML = st.view
    .map((i) => {
      const p = P[i], v = st.verdicts[i], k = decKind(v.decision);
      return `<div class="card ${k}" data-i="${i}" tabindex="0">
        <div class="th">${p.thumb ? `<img loading="lazy" src="${fileSrc(p.thumb)}" alt="">` : ""}
          <span class="sc ${cls(v.score)}">${v.score.toFixed(1)}</span>${v.overridden ? `<span class="man">sửa tay</span>` : ""}</div>
        <div class="cap"><div class="nm">${esc(p.name)}${v.time != null ? " · " + fmtTime(v.time) : ""}</div><div class="why">${esc(topWhy(p, v))}</div></div>
      </div>`;
    })
    .join("") || `<p class="note">Không có ảnh nào trong mục này.</p>`;
  for (const c of $("grid").children) {
    if (c.dataset.i != null) c.onclick = () => openDetail(+c.dataset.i);
  }
}

function wireResults() {
  for (const b of $("tabs").children) {
    b.setAttribute("aria-pressed", String(b.dataset.t === st.tab));
    b.onclick = () => {
      st.tab = b.dataset.t;
      for (const x of $("tabs").children) x.setAttribute("aria-pressed", String(x === b));
      renderGrid();
    };
  }
  $("reasonSel").onchange = () => { st.reason = $("reasonSel").value; if (st.reason) { st.tab = "all"; for (const x of $("tabs").children) x.setAttribute("aria-pressed", String(x.dataset.t === "all")); } renderGrid(); };
  $("sortSel").onchange = () => { st.sort = $("sortSel").value; renderGrid(); };
  $("btnExport").onclick = doExport;
}

async function doExport() {
  const def = await invoke("default_out_dir", { source: st.session.source });
  const ok = confirm(`Xuất ảnh vào:\n${def}\n\nẢnh gốc giữ nguyên. Bấm Huỷ để chọn thư mục khác.`);
  let out = def;
  if (!ok) {
    out = await pickFolder("Chọn nơi xuất ảnh");
    if (!out) return;
  }
  busy(true, "Đang xuất ảnh…");
  try {
    const r = await invoke("export_folder", { outDir: out });
    busy(false);
    renderProfile(r.profile);
    const rep = r.report;
    toast(
      `Đã xuất ${rep.copied} file vào thư mục đã lọc` +
        (rep.errors.length ? `, <b>${rep.errors.length} lỗi</b>: ${esc(rep.errors[0])}` : "") +
        ` · <a id="openOut">Mở thư mục</a>`,
      12000
    );
    const a = document.getElementById("openOut");
    if (a) a.onclick = () => T.opener.openPath(rep.out_dir);
  } catch (e) {
    busy(false);
    toast(esc(e));
  }
}

// ---------------------------------------------------------------- detail view

async function openDetail(i) {
  st.cur = i;
  $("detail").hidden = false;
  renderDetail();
}

async function renderDetail() {
  const i = st.cur, p = st.session.photos[i], v = st.verdicts[i];
  $("dname").textContent = p.name;
  $("dmeta").textContent = [p.camera, p.width ? `${p.width}×${p.height}` : "", v.time != null ? fmtTime(v.time) : "", p.files.length > 1 ? `${p.files.length} file` : ""].filter(Boolean).join(" · ");
  if (zoom.idx !== i) loadPhoto(i);
  const frame = $("dframe");
  for (const b of frame.querySelectorAll(".fbox")) b.remove();
  for (const f of p.faces) {
    const d = document.createElement("div");
    d.className = "fbox" + (f.closed ? " closed" : "");
    Object.assign(d.style, { left: f.bbox[0] * 100 + "%", top: f.bbox[1] * 100 + "%", width: f.bbox[2] * 100 + "%", height: f.bbox[3] * 100 + "%" });
    if (f.issues.length) d.innerHTML = `<span>${esc(f.issues[0])}</span>`;
    frame.appendChild(d);
  }
  const ai = v.overridden ? ` <span class="note">(AI: ${esc(decLabel(v.ai_decision))})</span>` : "";
  $("dscore").innerHTML = `<b class="${cls(v.score)}">${v.score.toFixed(1)}</b><span class="verdict">${esc(decLabel(v.decision))}${ai}</span>
    <ul>${v.notes.map((n) => `<li>${esc(n)}</li>`).join("")}${v.fixes.map((n) => `<li class="fix">Gợi ý sửa: ${esc(n)}</li>`).join("")}${
      v.learned != null ? `<li>AI đã học: khả năng bạn giữ ảnh này ${Math.round(v.learned * 100)}% (điểm theo quy tắc ${v.rule_score.toFixed(1)})</li>` : ""
    }${p.error ? `<li>${esc(p.error)}</li>` : ""}</ul>`;
  for (const b of document.querySelectorAll(".dact button")) {
    const k = b.dataset.d;
    b.classList.toggle("on", v.overridden ? k === v.decision.kind : k === "ai");
  }
  $("dcrit").innerHTML = p.scores
    ? p.notes
        .map(([k, notes]) => {
          const s = p.scores[k];
          if (s == null) return k === "people" ? `<div class="crit"><div class="h"><span>${CRIT_LABEL[k]}</span><span class="note">không có người</span></div></div>` : "";
          return `<div class="crit"><div class="h"><span>${CRIT_LABEL[k]}</span><b class="${cls(s)}">${s.toFixed(1)}</b></div><div class="bar"><i style="width:${s * 10}%"></i></div><ul>${notes
            .map((n) => `<li>${esc(n)}</li>`)
            .join("")}</ul></div>`;
        })
        .join("")
    : "";
}

// ---------------------------------------------------------------- zoom & pan

const zoom = { idx: -1, s: 1, tx: 0, ty: 0, full: false, fullLoading: null, drag: null, seq: 0 };
const Z_MAX = 8;

function loadPhoto(i) {
  const p = st.session.photos[i];
  const img = $("dimg");
  zoom.idx = i;
  zoom.full = false;
  zoom.fullLoading = null;
  const seq = ++zoom.seq;
  zoomTo(1, null);
  img.src = p.thumb ? fileSrc(p.thumb) : "";
  if (!p.error) {
    invoke("big_preview", { idx: i }).then((src) => { if (zoom.seq === seq && !zoom.full) img.src = fileSrc(src); }).catch(() => {});
  }
}

/** Load the preview at its native size once the user zooms past the 1800 px version. */
function ensureFull() {
  if (zoom.full || zoom.fullLoading || zoom.idx < 0 || st.session.photos[zoom.idx].error) return zoom.fullLoading || Promise.resolve();
  const seq = zoom.seq, img = $("dimg");
  $("zPct").textContent = "Đang tải…";
  zoom.fullLoading = invoke("big_preview", { idx: zoom.idx, full: true })
    .then((src) => new Promise((ok) => {
      if (zoom.seq !== seq) return ok();
      img.onload = () => { img.onload = null; zoom.full = true; showZoom(); ok(); };
      img.onerror = () => { img.onerror = null; ok(); };
      img.src = fileSrc(src);
    }))
    .catch(() => {})
    .finally(() => { zoom.fullLoading = null; showZoom(); });
  return zoom.fullLoading;
}

/** Scale at which one image pixel = one screen pixel. */
function scale100() {
  const img = $("dimg"), base = img.offsetWidth || 1;
  const p = st.session.photos[zoom.idx];
  const long = Math.max(img.naturalWidth, img.naturalHeight) || 1;
  // Before the full-size preview arrives, estimate it from the camera's pixel size.
  const fullLong = zoom.full ? long : Math.max(long, p ? Math.max(p.width, p.height) : 0);
  const natural = img.naturalWidth * (fullLong / long);
  return Math.max(1, natural / base / (window.devicePixelRatio || 1));
}

function showZoom() {
  const f = $("dframe");
  f.style.transform = `translate(${zoom.tx}px, ${zoom.ty}px) scale(${zoom.s})`;
  f.style.setProperty("--zs", zoom.s);
  $("dstage").classList.toggle("zoomed", zoom.s > 1.001);
  if (zoom.fullLoading) return;
  $("zPct").textContent = zoom.s <= 1.001 ? "Vừa khung" : Math.round((zoom.s / scale100()) * 100) + "%";
}

/** Zoom to scale `s` keeping the screen point (cx, cy) fixed; null = image centre. */
function zoomTo(s, at) {
  const f = $("dframe");
  s = Math.min(Math.max(s, 1), Z_MAX * Math.max(1, scale100()));
  if (s <= 1.001) {
    zoom.s = 1; zoom.tx = 0; zoom.ty = 0;
    return showZoom();
  }
  const r = f.getBoundingClientRect();
  const bx = r.left - zoom.tx, by = r.top - zoom.ty; // untransformed origin
  const cx = at ? at[0] : r.left + r.width / 2, cy = at ? at[1] : r.top + r.height / 2;
  const ux = (cx - bx - zoom.tx) / zoom.s, uy = (cy - by - zoom.ty) / zoom.s;
  zoom.tx = cx - bx - ux * s;
  zoom.ty = cy - by - uy * s;
  zoom.s = s;
  clampPan();
  showZoom();
  if (!zoom.full && zoom.s * $("dimg").offsetWidth > $("dimg").naturalWidth * 0.95) ensureFull();
}

/** Keep at least part of the image on screen. */
function clampPan() {
  const f = $("dframe"), stage = $("dstage").getBoundingClientRect();
  const w = f.offsetWidth * zoom.s, h = f.offsetHeight * zoom.s;
  const r = f.getBoundingClientRect();
  const bx = r.left - zoom.tx, by = r.top - zoom.ty;
  const minX = stage.right - 80 - bx - w, maxX = stage.left + 80 - bx;
  const minY = stage.bottom - 80 - by - h, maxY = stage.top + 80 - by;
  zoom.tx = Math.min(Math.max(zoom.tx, Math.min(minX, maxX)), Math.max(minX, maxX));
  zoom.ty = Math.min(Math.max(zoom.ty, Math.min(minY, maxY)), Math.max(minY, maxY));
}

async function zoom100(at) {
  if (zoom.s > 1.001 && Math.abs(zoom.s - scale100()) < 0.05) return zoomTo(1, null);
  await ensureFull();
  zoomTo(scale100(), at);
}

function wireZoom() {
  const stage = $("dstage");
  stage.addEventListener("wheel", (e) => {
    e.preventDefault();
    if (!e.ctrlKey && Math.abs(e.deltaX) > Math.abs(e.deltaY) * 0.5 && zoom.s > 1) {
      // Two-finger sideways / diagonal swipe on a trackpad: pan.
      zoom.tx -= e.deltaX; zoom.ty -= e.deltaY;
      clampPan(); showZoom();
      return;
    }
    const k = Math.exp(-e.deltaY * (e.ctrlKey ? 0.01 : 0.002));
    zoomTo(zoom.s * k, [e.clientX, e.clientY]);
  }, { passive: false });
  stage.addEventListener("pointerdown", (e) => {
    if (zoom.s <= 1.001 || e.button !== 0 || e.target.closest(".zoombar")) return;
    zoom.drag = { x: e.clientX, y: e.clientY, tx: zoom.tx, ty: zoom.ty };
    stage.setPointerCapture(e.pointerId);
    stage.classList.add("dragging");
  });
  stage.addEventListener("pointermove", (e) => {
    if (!zoom.drag) return;
    zoom.tx = zoom.drag.tx + e.clientX - zoom.drag.x;
    zoom.ty = zoom.drag.ty + e.clientY - zoom.drag.y;
    clampPan(); showZoom();
  });
  const end = () => { zoom.drag = null; stage.classList.remove("dragging"); };
  stage.addEventListener("pointerup", end);
  stage.addEventListener("pointercancel", end);
  stage.addEventListener("dblclick", (e) => { if (!e.target.closest(".zoombar")) zoom100([e.clientX, e.clientY]); });
  $("zIn").onclick = () => zoomTo(Math.max(zoom.s, 1) * 1.5, null);
  $("zOut").onclick = () => zoomTo(zoom.s / 1.5, null);
  $("zFit").onclick = () => zoomTo(1, null);
  $("z100").onclick = () => zoom100(null);
  window.addEventListener("resize", () => { if (!$("detail").hidden) zoomTo(1, null); });
}

async function decide(kind) {
  const i = st.cur;
  if (i < 0) return;
  let decision = null;
  if (kind === "selected") decision = { kind: "selected" };
  else if (kind === "review") decision = { kind: "review" };
  else if (kind === "rejected") {
    const cur = st.verdicts[i];
    decision = { kind: "rejected", reason: cur.ai_decision.kind === "rejected" ? cur.ai_decision.reason : "low-score" };
  }
  applyVerdicts(await invoke("set_decision", { idx: i, decision }));
}

function step(d) {
  const pos = st.view.indexOf(st.cur);
  const n = st.view[pos + d];
  if (n != null) openDetail(n);
}

function wireDetail() {
  $("dclose").onclick = () => { $("detail").hidden = true; st.cur = -1; zoom.idx = -1; };
  for (const b of document.querySelectorAll(".dact button")) b.onclick = () => decide(b.dataset.d === "ai" ? null : b.dataset.d);
  document.addEventListener("keydown", (e) => {
    if ($("detail").hidden || e.target.matches("input,select,textarea")) return;
    const k = e.key.toLowerCase();
    if (k === "escape") $("dclose").click();
    else if (k === "arrowright") step(1);
    else if (k === "arrowleft") step(-1);
    else if (k === "p") decide("selected");
    else if (k === "x") decide("rejected");
    else if (k === "u") decide(null);
    else if (k === "+" || k === "=") $("zIn").click();
    else if (k === "-" || k === "_") $("zOut").click();
    else if (k === "0") $("zFit").click();
    else if (k === "1" || k === "z") $("z100").click();
  });
}

// ---------------------------------------------------------------- boot

(async function boot() {
  st.info = await invoke("app_info");
  st.prefs = st.info.prefs;
  renderMeta();
  renderPresets();
  renderWeights();
  renderRules();
  renderOptions();
  renderProfile();
  wireRail();
  wireIn();
  wireResults();
  wireDetail();
  wireZoom();
})();
