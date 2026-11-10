// SonVu Raw Pick UI. Talks to the Rust side through Tauri commands; all heavy work happens there.
"use strict";

const T = window.__TAURI__;
// One command on the Rust side, shared by the window and the web version.
const invoke = (cmd, args) => T.core.invoke("call", { cmd, args: args || {} });
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
  ["review_open_mouth", "Ảnh nhân vật chính đang há miệng đưa vào «Cần xem lại»"],
  ["dedupe", "Lọc ảnh trùng, giữ ảnh đẹp nhất mỗi loạt"],
  ["use_learned", "Dùng điều AI đã học từ lựa chọn của bạn"],
];

const st = {
  info: null, prefs: null, profile: null,
  // Sessions: one folder each. `views` caches the full data of analysed sessions.
  sid: null, metas: [], views: {}, progress: {},
  session: null, verdicts: [], summary: null,
  tab: "all", reason: "", sort: "time", view: [], cur: -1,
};
const meta = (id = st.sid) => st.metas.find((m) => m.id === id);

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

/**
 * In-page question box. The app's webview (macOS above all) silently refuses the browser's own
 * confirm() and prompt(), so every question goes through this.
 * With `value` it asks for text and resolves to the text (or null); otherwise to true/false.
 */
function ask(text, { value = null, ok = "Đồng ý", title = "", cancel = "Huỷ" } = {}) {
  return new Promise((resolve) => {
    const wrap = document.createElement("div");
    wrap.className = "modal ask";
    const withInput = value !== null;
    wrap.innerHTML = `<form class="box">
      ${title ? `<h3>${esc(title)}</h3>` : ""}
      <p class="asktext">${esc(text)}</p>
      ${withInput ? `<input class="askin" spellcheck="false" aria-label="${esc(title || text)}">` : ""}
      <div class="row2 end"><button type="button" class="btn" data-no>${esc(cancel)}</button><button type="submit" class="btn primary">${esc(ok)}</button></div>
    </form>`;
    const input = wrap.querySelector(".askin");
    if (input) input.value = value;
    const done = (v) => { wrap.remove(); document.removeEventListener("keydown", onKey, true); resolve(v); };
    const onKey = (e) => { if (e.key === "Escape") { e.stopPropagation(); done(withInput ? null : false); } };
    wrap.querySelector("form").onsubmit = (e) => { e.preventDefault(); done(withInput ? input.value : true); };
    wrap.querySelector("[data-no]").onclick = () => done(withInput ? null : false);
    wrap.onclick = (e) => { if (e.target === wrap) done(withInput ? null : false); };
    document.addEventListener("keydown", onKey, true);
    document.body.append(wrap);
    (input || wrap.querySelector("[type=submit]")).focus();
    if (input) input.select();
  });
}
function cls(v) { return v >= 7.5 ? "hi" : v >= 5 ? "mid" : "lo"; }
function decKind(d) { return d.kind; }
/** AI picking switched off: nothing is chosen until Sơn chooses. */
const aiOff = () => st.prefs && st.prefs.settings.options.ai_pick === false;

function decLabel(d) {
  if (d.kind === "selected") return "Đã chọn";
  if (d.kind === "review") return aiOff() ? "Chưa chọn" : "Cần xem lại";
  return "Bị loại: " + (REASONS[d.reason] || d.reason);
}
function fmtTime(t) {
  if (t == null) return "";
  const d = new Date((t + 946684800) * 1000);
  return d.toISOString().slice(11, 19);
}

// Modal progress, only for the memory-card copy; sessions show progress in their own tab.
let busyOn = false;
function busy(on, text) {
  busyOn = on;
  $("busy").hidden = !on;
  if (text) $("busyText").textContent = text;
  $("busyBar").style.width = "0%";
  $("busyCount").textContent = "";
}
T.event.listen("progress", (e) => {
  const { session, done, total, stage } = e.payload;
  if (session == null) {
    if (!busyOn) return;
    $("busyText").textContent = stage + "…";
    $("busyBar").style.width = total ? (100 * done) / total + "%" : "0%";
    $("busyCount").textContent = `${done} / ${total}`;
    return;
  }
  st.progress[session] = { done, total, stage };
  renderTabs();
  if (session === st.sid) renderProgress();
});
T.event.listen("sessions-changed", async () => {
  st.metas = await invoke("list_sessions");
  renderTabs();
  renderMain();
});
$("btnCancel").onclick = () => invoke("cancel", { id: null });

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
  $("expReport").checked = st.prefs.export.report;
  renderExpMode();
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
  if (p) st.profile = p;
  p = st.profile;
  if (!p) return;
  const sel = $("profileSel");
  if (!st.info.profiles.includes(st.prefs.profile)) st.info.profiles.unshift(st.prefs.profile);
  sel.innerHTML = st.info.profiles.map((n) => `<option ${n === st.prefs.profile ? "selected" : ""}>${esc(n)}</option>`).join("");
  const pct = (v) => (v == null ? "—" : Math.round(v * 100) + "%");
  $("profileBox").innerHTML = p.ready
    ? `Đã học từ <b>${p.samples}</b> ảnh (<b>${p.manual || 0}</b> ảnh bạn tự chọn${p.seen ? `, nhìn hình <b>${p.seen}</b> ảnh` : ""}) · tái hiện đúng <b>${pct(p.accuracy)}</b> lựa chọn của bạn · gu của bạn chiếm <b>${pct(p.trust)}</b> điểm${p.shared ? " · học chung với máy khác" : ""}<ul>${p.explain.map((e) => `<li>${esc(e)}</li>`).join("")}</ul>
       <p class="note">Thấy AI chọn sai thì bấm Chọn (P) hoặc Loại (X): AI học ngay từng lần bấm.</p>`
    : `Đang học gu của bạn: <b>${p.samples}</b> / ${p.min_samples} lựa chọn (cần cả ảnh giữ lẫn ảnh bỏ).
       <p class="note">Mở ảnh AI chọn sai, bấm Chọn (P) hoặc Loại (X). Mỗi lần bấm AI học ngay, đủ ${p.min_samples} lần là bắt đầu chấm theo gu bạn.</p>`;
  $("btnTrain").disabled = !(st.session && meta()?.phase === "ready");
}

function renderExpMode() {
  const mv = !!st.prefs.export.move_files;
  for (const b of $("expMode").children) b.setAttribute("aria-checked", String((b.dataset.m === "move") === mv)), b.setAttribute("aria-pressed", String((b.dataset.m === "move") === mv));
  $("expNote").textContent = mv
    ? "Ảnh được chuyển sang thư mục bạn chọn khi bấm Xuất. Bản gốc chỉ bị xoá sau khi bản mới đã kiểm tra khớp từng byte; cùng ổ đĩa thì chỉ đổi chỗ, không ghi lại file."
    : "Ảnh gốc giữ nguyên. File RAW và JPEG được chép nguyên bản, kiểm tra checksum từng file.";
}

let saveTimer = 0;
function savePrefs() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(async () => {
    const v = await invoke("save_prefs", { id: st.sid, prefs: st.prefs });
    const m = meta();
    if (m) m.prefs = JSON.parse(JSON.stringify(st.prefs));
    if (v && st.session) applyVerdicts(v);
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
  $("expReport").onchange = () => { st.prefs.export.report = $("expReport").checked; savePrefs(); };
  for (const b of $("expMode").children) {
    b.onclick = () => { st.prefs.export.move_files = b.dataset.m === "move"; renderExpMode(); savePrefs(); };
  }
  $("profileSel").onchange = async () => {
    st.prefs.profile = $("profileSel").value;
    renderProfile(await invoke("profile_info", { name: st.prefs.profile }));
    savePrefs();
  };
  $("btnNewProfile").onclick = async () => {
    const n = await ask("Ví dụ: Cưới, Hội nghị, Sân khấu. AI học riêng cho từng loại sự kiện.", { value: "", title: "Thêm loại sự kiện", ok: "Thêm" });
    if (!n || !n.trim()) return;
    st.prefs.profile = n.trim();
    st.info.profiles.unshift(st.prefs.profile);
    invoke("profile_info", { name: st.prefs.profile }).then(renderProfile);
    savePrefs();
  };
  $("btnTrain").onclick = async () => {
    try {
      const p = await invoke("train", { id: st.sid });
      renderProfile(p);
      toast(p.ready ? `AI đã học xong, tái hiện đúng ${Math.round(p.accuracy * 100)}% lựa chọn của bạn.` : `Đã lưu ${p.samples} lựa chọn. Cần ${p.min_samples} ảnh có cả giữ và bỏ để AI bắt đầu học.`);
    } catch (e) { toast(esc(e)); }
  };
}

// ---------------------------------------------------------------- folder in

// ---------------------------------------------------------------- sessions

function sessionLabel(m) {
  const pr = st.progress[m.id];
  if (m.phase === "waiting") return "đang chờ";
  if (m.phase === "analyzing" || m.phase === "exporting") return pr && pr.total ? Math.round((100 * pr.done) / pr.total) + "%" : "…";
  if (m.phase === "failed") return "lỗi";
  if (m.summary) return `${m.summary.selected}/${m.summary.total}`;
  return "trống";
}

function renderTabs() {
  const box = $("stabs");
  box.innerHTML =
    st.metas
      .map((m) => `<div class="stab ${m.id === st.sid ? "on" : ""} ph-${m.phase}" data-id="${m.id}" title="${esc(m.source || "Chưa có thư mục")}">
        <span class="nm">${esc(m.source ? m.name : "Phiên mới")}</span><span class="st">${sessionLabel(m)}</span>
        <button class="x" data-close="${m.id}" aria-label="Đóng phiên">×</button></div>`)
      .join("") + `<button class="btn sm" id="btnNewSession" title="Mở thêm một thư mục khác">+ Phiên mới</button>`;
  for (const t of box.querySelectorAll(".stab")) t.onclick = (e) => { if (!e.target.dataset.close) selectSession(+t.dataset.id); };
  for (const x of box.querySelectorAll("[data-close]")) x.onclick = () => closeSession(+x.dataset.close);
  $("btnNewSession").onclick = async () => selectSession((await newSession()).id);
}

async function newSession() {
  const m = await invoke("new_session");
  if (!st.metas.find((x) => x.id === m.id)) st.metas.push(m);
  renderTabs();
  return m;
}

async function closeSession(id) {
  const m = meta(id);
  if (m && m.source && !(await ask(`Ảnh và kết quả đã xuất không bị ảnh hưởng.`, { title: `Đóng phiên «${m.name}»?`, ok: "Đóng phiên" }))) return;
  await invoke("close_session", { id });
  st.metas = st.metas.filter((x) => x.id !== id);
  delete st.views[id];
  if (!st.metas.length) await newSession();
  if (st.sid === id || !meta()) selectSession(st.metas[st.metas.length - 1].id);
  else renderTabs();
}

async function selectSession(id) {
  st.sid = id;
  st.cur = -1;
  $("detail").hidden = true;
  const m = meta(id);
  st.prefs = JSON.parse(JSON.stringify(m ? m.prefs : st.info.defaults));
  if (m && m.phase === "ready" && !st.views[id]) {
    try { st.views[id] = await invoke("get_session", { id }); } catch (e) { /* closed meanwhile */ }
  }
  renderTabs();
  renderPresets();
  renderWeights();
  renderRules();
  renderOptions();
  invoke("profile_info", { name: st.prefs.profile }).then(renderProfile);
  renderMain();
}

/** Show the right panel for the current session's phase. */
function renderMain() {
  const m = meta();
  if (!m) return;
  const view = st.views[m.id];
  const ready = (m.phase === "ready" || m.phase === "exporting") && view;
  $("empty").hidden = !(m.phase === "empty" || (m.phase === "failed" && !view));
  $("emptyErr").hidden = m.phase !== "failed";
  $("emptyErr").textContent = m.error || "";
  $("results").hidden = !ready;
  renderProgress();
  if (ready) {
    if (st.session !== view) {
      st.session = view;
      st.verdicts = view.verdicts;
      st.summary = view.summary;
      st.reason = "";
      renderSummary();
      renderGrid();
    }
  } else {
    st.session = null;
  }
  $("btnExport").disabled = m.phase !== "ready";
  $("btnExportOther").disabled = m.phase !== "ready";
  $("btnTrain").disabled = m.phase !== "ready";
  renderAiSwitch();
}

function renderAiSwitch() {
  $("optAiPick").checked = !aiOff();
  const t = [...$("tabs").children].find((b) => b.dataset.t === "review");
  if (t) t.textContent = aiOff() ? "Chưa chọn" : "Cần xem lại";
}

function renderProgress() {
  const m = meta();
  const run = m && ["waiting", "analyzing", "exporting"].includes(m.phase);
  $("sprog").hidden = !run;
  if (!run) return;
  const pr = st.progress[m.id] || { done: 0, total: 0, stage: "" };
  $("sprogText").textContent = m.phase === "waiting" ? "Đang chờ phiên khác phân tích xong (máy chạy từng thư mục một để không bị chậm)…" : (pr.stage || "Đang đọc thư mục") + "…";
  $("sprogBar").style.width = pr.total ? (100 * pr.done) / pr.total + "%" : "0%";
  $("sprogCount").textContent = pr.total ? `${pr.done} / ${pr.total} · ${esc(m.source || "")}` : esc(m.source || "");
}

/** Analyse `path` in session `id` (a new session if omitted and the current one is busy). */
async function openFolder(path, id) {
  if (!path) return;
  if (id == null) {
    const cur = meta();
    id = cur && cur.phase === "empty" ? cur.id : (await newSession()).id;
  }
  if (st.sid !== id) await selectSession(id);
  const m = meta(id);
  if (m) { m.phase = "waiting"; m.source = path; m.name = path.split(/[\\/]/).filter(Boolean).pop() || path; }
  renderTabs();
  renderMain();
  try {
    const v = await invoke("analyze", { id, source: path });
    st.views[id] = v;
    Object.assign(meta(id) || {}, v.meta);
    if (st.sid === id) {
      st.prefs = JSON.parse(JSON.stringify(v.meta.prefs));
      renderRules();
      renderOptions();
      renderProfile(v.profile);
      renderMain();
      if (v.applied.length) toast("AI đã tự bật: " + v.applied.map(esc).join(", "));
    }
    renderTabs();
  } catch (e) {
    if (String(e) !== "Đã dừng") toast(esc(e));
    st.metas = await invoke("list_sessions");
    renderTabs();
    renderMain();
  }
}

/** Reopen saved sessions after a restart (from the cache, one after another). */
async function restoreSessions() {
  for (const m of st.metas.filter((x) => x.source)) {
    try {
      const v = await invoke("analyze", { id: m.id, source: null });
      st.views[m.id] = v;
      Object.assign(meta(m.id) || {}, v.meta);
      if (st.sid === m.id) renderMain();
      renderTabs();
    } catch (e) { /* folder moved or deleted: the tab shows the error */ }
  }
}

async function pickFolder(title) {
  return await T.dialog.open({ directory: true, multiple: false, title });
}

function wireIn() {
  const drop = $("drop");
  drop.onclick = async () => openFolder(await pickFolder("Chọn thư mục ảnh"));
  $("btnPickMain").onclick = async () => openFolder(await pickFolder("Chọn thư mục ảnh cho phiên này"), st.sid);
  $("sprogStop").onclick = () => invoke("cancel", { id: st.sid });
  drop.onkeydown = (e) => { if (e.key === "Enter" || e.key === " ") drop.click(); };
  T.webview.getCurrentWebview().onDragDropEvent((e) => {
    const t = e.payload.type;
    if (t === "over" || t === "enter") drop.classList.add("over");
    else drop.classList.remove("over");
    if (t === "drop" && e.payload.paths?.length) {
      // Several folders dropped at once: one session each.
      const [first, ...rest] = e.payload.paths;
      openFolder(first);
      for (const p of rest) newSession().then((m) => openFolder(p, m.id));
    }
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
  if (v.profile) renderProfile(v.profile);
  st.verdicts = v.verdicts;
  st.summary = v.summary;
  if (st.session) { st.session.verdicts = v.verdicts; st.session.summary = v.summary; }
  const m = meta();
  if (m) { m.summary = v.summary; renderTabs(); }
  renderSummary();
  renderGrid();
  if (st.cur >= 0) renderDetail();
}

function renderSummary() {
  const s = st.summary, t = st.session.timing || { seconds: 0, per_photo_ms: 0, from_cache: 0 };
  const secs = t.seconds < 60 ? `${t.seconds.toFixed(1)} giây` : `${(t.seconds / 60).toFixed(1)} phút`;
  $("summary").innerHTML = `
    <div class="stat pick"><b>${s.selected}</b><span>đã chọn</span></div>
    <div class="stat"><b>${s.review}</b><span>${aiOff() ? "chưa chọn" : "cần xem lại"}</span></div>
    <div class="stat"><b>${s.rejected}</b><span>bị loại</span></div>
    <div class="stat"><b>${s.total}</b><span>ảnh${s.moments > 1 ? ` · ${s.moments} khoảnh khắc` : ""}</span></div>
    <p class="meta">${esc(st.session.meta.source)} · phân tích trong ${secs} (${Math.round(t.per_photo_ms)} ms/ảnh${t.from_cache ? `, ${t.from_cache} ảnh lấy lại từ lần trước` : ""})</p>`;
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
  if (v.decision.kind === "review" && aiOff() && !v.overridden) return v.ai_decision.kind === "selected" ? "AI gợi ý: chọn" : v.ai_decision.kind === "rejected" ? "AI gợi ý: loại · " + (REASONS[v.ai_decision.reason] || "") : "AI gợi ý: xem lại";
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
  $("btnExport").onclick = doExportShow;
  $("btnLearnShow").onclick = doLearnShow;
  $("btnExportOther").onclick = doExport;
  $("optAiPick").onchange = () => {
    st.prefs.settings.options.ai_pick = $("optAiPick").checked;
    renderAiSwitch();
    savePrefs();
    toast($("optAiPick").checked ? "AI tự chọn lại ảnh. Ảnh bạn đã chọn tay giữ nguyên." : "AI vẫn chấm điểm từng ảnh nhưng không chọn sẵn. Bạn tự chọn bằng phím P (chọn) và X (loại).", 7000);
  };
}

/** One click (Sơn's flow): archive into «<source>_anh chon» inside the show, then copy to his Google Drive folder. */
async function doExportShow() {
  const id = st.sid, m = meta(id);
  if (!m || m.phase !== "ready") return;
  if (!st.summary.selected) return toast("Chưa có ảnh nào được chọn.");
  if (!st.info.drive_dir) {
    const ok = await ask("Chọn thư mục trên Google Drive để gửi ảnh cho khách. App sẽ nhớ thư mục này, lần sau xuất thẳng không hỏi lại.\n\nBấm «Để sau» nếu lần này chỉ lưu vào thư mục gốc.", { title: "Thư mục Google Drive", ok: "Chọn thư mục Drive", cancel: "Để sau" });
    if (ok) await pickDriveDir();
  }
  m.phase = "exporting";
  st.progress[id] = { done: 0, total: 0, stage: "Đang lưu ảnh chọn vào thư mục gốc" };
  renderTabs();
  renderMain();
  try {
    const r = await invoke("export_show", { id });
    m.phase = "ready";
    renderProfile(r.profile);
    const rep = r.report;
    let msg = `Đã lưu ${rep.copied + rep.moved + rep.skipped_same} file vào <a id="openArc">${esc(rep.out_dir)}</a>` + (rep.errors.length ? `, <b>${rep.errors.length} lỗi</b>: ${esc(rep.errors[0])}` : "") + ".";
    if (r.drive) {
      msg += `<br>Đã chép lên Google Drive, app Drive đang tải lên. `;
      msg += r.drive.link
        ? `Link: <a id="copyLink">${esc(r.drive.link)}</a> (đã chép, dán gửi khách được ngay).`
        : `<a id="openDrive">Mở thư mục trên máy</a>, chuột phải vào thư mục → Google Drive → «Sao chép đường liên kết» để gửi khách.`;
    } else if (r.drive_error) {
      msg += `<br><b>Chưa lên Drive:</b> ${esc(r.drive_error)}`;
    }
    toast(msg, 30000);
    const arc = document.getElementById("openArc");
    if (arc) arc.onclick = () => T.opener.openPath(rep.out_dir);
    if (r.drive && r.drive.link) {
      try { await navigator.clipboard.writeText(r.drive.link); } catch (e) { /* clipboard blocked: the link is shown */ }
      const a = document.getElementById("copyLink");
      if (a) a.onclick = () => navigator.clipboard.writeText(r.drive.link).then(() => toast("Đã chép link."), () => {});
    }
    const od = document.getElementById("openDrive");
    if (od && r.drive) od.onclick = () => T.opener.openPath(r.drive.dir.replace(/[\\/][^\\/]+[\\/]?$/, ""));
  } catch (e) {
    m.phase = "ready";
    toast(esc(e));
  }
  st.metas = await invoke("list_sessions");
  renderTabs();
  renderMain();
}

async function pickDriveDir() {
  const def = st.info.drive_dir || (st.info.drive_roots || [])[0];
  const d = await T.dialog.open({ directory: true, multiple: false, title: "Chọn thư mục trên Google Drive để gửi ảnh cho khách", defaultPath: def || undefined });
  if (!d) return false;
  try {
    st.info = Object.assign(st.info, await invoke("set_drive_dir", { path: d }));
    renderDrive();
    return true;
  } catch (e) {
    toast(esc(e));
    return false;
  }
}

function renderDrive() {
  const d = st.info.drive_dir;
  $("driveBox").innerHTML = d
    ? `<span>Xuất xong chép lên Google Drive:</span><span class="path">${esc(d)}</span>
       <span class="row"><button class="btn sm" id="driveChange">Đổi thư mục Drive</button><button class="btn sm" id="driveOff">Thôi chép lên Drive</button></span>`
    : `<span>Chưa chọn thư mục Google Drive. Bấm «Xuất ảnh chọn» lần đầu app sẽ hỏi, hoặc chọn ngay:</span>
       <span class="row"><button class="btn sm" id="driveChange">Chọn thư mục Drive…</button></span>`;
  $("driveChange").onclick = pickDriveDir;
  const off = document.getElementById("driveOff");
  if (off) off.onclick = async () => { st.info = Object.assign(st.info, await invoke("set_drive_dir", { path: null })); renderDrive(); };
}

function renderSync() {
  const d = st.info.sync_dir;
  $("syncBox").innerHTML = d
    ? `<span>Dùng chung điều AI đã học với máy khác qua:</span><span class="path">${esc(d)}</span>
       <span class="row"><button class="btn sm" id="syncChange">Đổi thư mục</button><button class="btn sm" id="syncOff">Thôi dùng chung</button></span>`
    : `<span>Làm trên cả máy Win và Mac? Chọn cùng một thư mục trên Google Drive ở mỗi máy để hai máy học chung.</span>
       <span class="row"><button class="btn sm" id="syncChange">Chọn thư mục học chung…</button></span>`;
  $("syncChange").onclick = async () => {
    const def = st.info.sync_dir || (st.info.drive_roots || [])[0];
    const dir = await T.dialog.open({ directory: true, multiple: false, title: "Thư mục trên Google Drive để các máy học chung (chọn cùng thư mục trên mọi máy)", defaultPath: def || undefined });
    if (!dir) return;
    try {
      st.info = Object.assign(st.info, await invoke("set_sync_dir", { path: dir }));
      renderSync();
      renderProfile(await invoke("profile_info", { name: st.prefs.profile }));
      toast("Đã bật học chung. Máy kia chọn cùng thư mục này là hai máy dùng chung điều AI đã học.", 9000);
    } catch (e) {
      toast(esc(e));
    }
  };
  const off = document.getElementById("syncOff");
  if (off) off.onclick = async () => { st.info = Object.assign(st.info, await invoke("set_sync_dir", { path: null })); renderSync(); };
}

async function doLearnShow() {
  const dir = await T.dialog.open({ directory: true, multiple: false, title: "Chọn thư mục một show cũ bạn đã lọc xong" });
  if (!dir) return;
  busy(true, "Đang xem ảnh show cũ…");
  try {
    const r = await invoke("learn_show", { path: dir, name: st.prefs.profile });
    busy(false);
    renderProfile(r.profile);
    const l = r.lesson;
    toast(
      l.only_picks
        ? `Không thấy ảnh gốc trong thư mục này, nên app coi cả ${l.kept} ảnh là ảnh bạn đã chọn và học theo. Học thêm show có cả ảnh gốc để AI biết bạn hay bỏ ảnh kiểu nào.`
        : `Đã học ${l.kept + l.dropped} ảnh của show: bạn chọn ${l.kept}, bỏ ${l.dropped}` + (l.pick_folders.length ? ` (ảnh chọn nằm ở ${l.pick_folders.map((f) => "«" + esc(f.split(/[\\/]/).pop()) + "»").join(", ")})` : "") + `. AI áp dụng ngay cho loại sự kiện «${esc(st.prefs.profile)}».`,
      15000
    );
    if (st.sid) { const v = await invoke("get_session", { id: st.sid }).catch(() => null); if (v) { st.views[st.sid] = v; st.session = null; renderMain(); } }
  } catch (e) {
    busy(false);
    toast(esc(e));
  }
}

async function doExport() {
  const id = st.sid, m = meta(id);
  if (!m || m.phase !== "ready") return;
  const src = m.source || "";
  const defParent = st.info.export_parent || src.replace(/[\\/][^\\/]+[\\/]?$/, "");
  const parent = await T.dialog.open({ directory: true, multiple: false, title: "Chọn thư mục để xuất ảnh đã lọc", defaultPath: defParent || undefined });
  if (!parent) return;
  const target = await invoke("export_target", { id, parent });
  const mv = st.prefs.export.move_files;
  const what = [`${st.summary.selected} ảnh đã chọn`, st.prefs.export.copy_review ? `${st.summary.review} ảnh cần xem lại` : "", st.prefs.export.copy_rejected ? `${st.summary.rejected} ảnh bị loại` : ""].filter(Boolean).join(", ");
  const msg = mv
    ? `CHUYỂN ${what} (kèm file RAW) sang:\n${target}\n\nẢnh gốc sẽ bị xoá khỏi thư mục cũ, chỉ sau khi bản mới đã kiểm tra khớp từng byte.`
    : `Chép ${what} (kèm file RAW, giữ nguyên chất lượng) vào:\n${target}\n\nẢnh gốc giữ nguyên.`;
  if (!(await ask(msg, { title: mv ? "Chuyển ảnh sang thư mục mới" : "Xuất ảnh", ok: mv ? "Chuyển ảnh" : "Chép ảnh" }))) return;
  st.info.export_parent = parent;
  m.phase = "exporting";
  st.progress[id] = { done: 0, total: 0, stage: "Đang xuất ảnh" };
  renderTabs();
  renderMain();
  try {
    const r = await invoke("export_folder", { id, parent });
    m.phase = "ready";
    renderProfile(r.profile);
    const rep = r.report;
    const n = rep.moved || rep.copied;
    toast(
      `${mv ? "Đã chuyển" : "Đã chép"} ${n} file (${(rep.bytes / 1e9).toFixed(2)} GB) vào ${esc(target)}` +
        (rep.skipped_same ? `, bỏ qua ${rep.skipped_same} file đã có` : "") +
        (rep.errors.length ? `, <b>${rep.errors.length} lỗi</b>: ${esc(rep.errors[0])}` : "") +
        ` · <a id="openOut">Mở thư mục</a>`,
      15000
    );
    const a = document.getElementById("openOut");
    if (a) a.onclick = () => T.opener.openPath(rep.out_dir);
  } catch (e) {
    m.phase = "ready";
    toast(esc(e));
  }
  st.metas = await invoke("list_sessions");
  renderTabs();
  renderMain();
}

/** Copy the selected shots into a sub-folder of the source folder; originals stay as they are. */
async function doGather() {
  const id = st.sid, m = meta(id);
  if (!m || m.phase !== "ready") return;
  const sub = await ask(`Chép ${st.summary.selected} ảnh đã chọn (kèm file RAW, JPEG, XMP cùng tên) sang thư mục con trong:\n${m.source}\n\nẢnh gốc giữ nguyên tại chỗ. Bản chép giống hệt từng byte, có kiểm tra checksum. Tên thư mục con:`, { value: "Anh-da-chon", title: "Chép ảnh đã chọn vào thư mục con", ok: "Chép ảnh" });
  if (!sub || !sub.trim()) return;
  m.phase = "exporting";
  st.progress[id] = { done: 0, total: 0, stage: "Đang chép ảnh đã chọn vào thư mục con" };
  renderTabs();
  renderMain();
  try {
    const r = await invoke("gather_selected", { id, sub: sub.trim() });
    st.views[id] = r.view;
    Object.assign(m, r.view.meta);
    const rep = r.report;
    if (!rep.files && !rep.skipped && rep.errors.length) {
      toast(`<b>Chưa chép được ảnh nào.</b> ${esc(rep.errors[0])}`, 30000);
    } else {
      toast(`Đã chép ${rep.photos} ảnh (${rep.files} file, ${(rep.bytes / 1e9).toFixed(2)} GB) vào ${esc(rep.dir)}` +
        (rep.skipped ? `, ${rep.skipped} file đã có sẵn nên bỏ qua` : "") + `. Ảnh gốc giữ nguyên.` +
        (rep.errors.length ? ` <b>${rep.errors.length} lỗi</b>: ${esc(rep.errors[0])}` : "") + ` · <a id="openOut">Mở thư mục</a>`, 15000);
      const a = document.getElementById("openOut");
      if (a) a.onclick = () => T.opener.openPath(rep.dir);
    }
  } catch (e) {
    toast(esc(e));
  }
  m.phase = "ready";
  st.metas = await invoke("list_sessions");
  renderTabs();
  renderMain();
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
  const mains = p.faces.map((f, n) => ({ f, n })).filter((x) => x.f.main);
  p.faces.forEach((f, n) => {
    const d = document.createElement("div");
    d.className = "fbox" + (f.closed ? " closed" : "");
    d.dataset.n = n;
    Object.assign(d.style, { left: f.bbox[0] * 100 + "%", top: f.bbox[1] * 100 + "%", width: f.bbox[2] * 100 + "%", height: f.bbox[3] * 100 + "%" });
    const num = mains.length > 1 ? mains.findIndex((x) => x.n === n) + 1 : 0;
    const label = [num > 0 ? `${num}` : "", f.issues[0] || ""].filter(Boolean).join(" · ");
    if (label) d.innerHTML = `<span>${esc(label)}</span>`;
    frame.appendChild(d);
  });
  renderEyes(i, p, mains);
  const ai = v.overridden || (aiOff() && v.ai_decision.kind !== "review")
    ? ` <span class="note">(AI ${v.overridden ? "" : "gợi ý "}: ${esc(aiOff() && v.ai_decision.kind === "review" ? "Cần xem lại" : decLabel(v.ai_decision))})</span>`
    : "";
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

const zoom = { idx: -1, s: 1, tx: 0, ty: 0, bw: 1, bh: 1, full: false, fullLoading: null, drag: null, seq: 0 };
const Z_MAX = 8;

/** Large previews already made (and warmed in the browser), by session and photo. */
const bigCache = new Map();
/** Previews being made run two at a time; the photo on screen jumps the queue. */
const bigQueue = [];
let bigRunning = 0;

function bigPump() {
  while (bigRunning < 2 && bigQueue.length) {
    const job = bigQueue.shift();
    bigRunning++;
    invoke("big_preview", { id: job.sid, idx: job.i })
      .then((src) => {
        const url = fileSrc(src);
        const im = new Image();
        im.src = url;
        return (im.decode ? im.decode() : Promise.resolve()).then(() => url, () => url);
      })
      .then(job.ok, job.fail)
      .finally(() => { bigRunning--; bigPump(); });
  }
}

function bigPreview(i, urgent) {
  const key = `${st.sid}:${i}`;
  if (!bigCache.has(key)) {
    let job;
    const pr = new Promise((ok, fail) => { job = { sid: st.sid, i, key, ok, fail }; });
    pr.catch(() => bigCache.delete(key));
    bigCache.set(key, pr);
    if (urgent) bigQueue.unshift(job); else bigQueue.push(job);
    if (bigCache.size > 80) bigCache.delete(bigCache.keys().next().value);
    bigPump();
  } else if (urgent) {
    // Already waiting behind prefetches: move it to the front.
    const at = bigQueue.findIndex((j) => j.key === key);
    if (at > 0) bigQueue.unshift(bigQueue.splice(at, 1)[0]);
  }
  return bigCache.get(key);
}

/** Get the next photos (and the one before) ready, so stepping through never waits. */
function prefetchAround(i) {
  const pos = st.view.indexOf(i);
  // Forget prefetches for photos we have stepped away from.
  for (let k = bigQueue.length - 1; k >= 0; k--) {
    const j = bigQueue[k];
    const d = st.view.indexOf(j.i) - pos;
    if (j.sid !== st.sid || d < -2 || d > 6) { bigQueue.splice(k, 1); bigCache.delete(j.key); j.fail(new Error("skip")); }
  }
  for (const d of [1, 2, -1, 3, 4, 5, -2]) {
    const n = st.view[pos + d];
    if (n != null && !st.session.photos[n].error) bigPreview(n).catch(() => {});
  }
}

function loadPhoto(i) {
  const p = st.session.photos[i];
  const img = $("dimg");
  zoom.idx = i;
  zoom.full = false;
  zoom.fullLoading = null;
  const seq = ++zoom.seq;
  zoomTo(1, null); // the frame already has the photo's fitted size, whatever is drawn in it
  if (p.error) {
    img.src = p.thumb ? fileSrc(p.thumb) : "";
    return;
  }
  const big = bigPreview(i, true);
  let shown = false;
  big.then((url) => { if (zoom.seq === seq && !zoom.full) { shown = true; img.src = url; } }).catch(() => {});
  // Start on the neighbours straight away (they queue behind this one).
  prefetchAround(i);
  // Ready in a moment? Skip the thumbnail flash; otherwise show it stretched to size meanwhile.
  setTimeout(() => { if (!shown && zoom.seq === seq && p.thumb) img.src = fileSrc(p.thumb); }, 60);
}

/** Size of the photo fitted into the viewer, from its real proportions. */
function fitSize() {
  const stage = $("dstage"), img = $("dimg");
  const p = zoom.idx >= 0 ? st.session.photos[zoom.idx] : null;
  const aw = Math.max(1, stage.clientWidth - 20), ah = Math.max(1, stage.clientHeight - 20);
  const ratio = p && p.width && p.height ? p.width / p.height : (img.naturalWidth || 3) / (img.naturalHeight || 2);
  const w = Math.min(aw, ah * ratio);
  return [Math.round(w), Math.round(w / ratio)];
}

/** Load the preview at its native size once the user zooms past the 2560 px version. */
function ensureFull() {
  if (zoom.full || zoom.fullLoading || zoom.idx < 0 || st.session.photos[zoom.idx].error) return zoom.fullLoading || Promise.resolve();
  const seq = zoom.seq, img = $("dimg");
  $("zPct").textContent = "Đang tải…";
  zoom.fullLoading = invoke("big_preview", { id: st.sid, idx: zoom.idx, full: true })
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

/** Width of the image when it fits the stage (zoom 1). */
function baseWidth() {
  return zoom.s > 1.001 ? zoom.bw : ($("dimg").offsetWidth || 1);
}

/** Scale at which one image pixel = one screen pixel. */
function scale100() {
  const img = $("dimg"), base = baseWidth();
  const p = st.session.photos[zoom.idx];
  const long = Math.max(img.naturalWidth, img.naturalHeight) || 1;
  // Before the full-size preview arrives, estimate it from the camera's pixel size.
  const fullLong = zoom.full ? long : Math.max(long, p ? Math.max(p.width, p.height) : 0);
  const natural = img.naturalWidth * (fullLong / long);
  return Math.max(1, natural / base / (window.devicePixelRatio || 1));
}

/**
 * Zoomed, the image is laid out at its real zoomed size and only moved with translate().
 * Scaling it with transform: scale() looks smooth but WebKit (macOS) then enlarges a bitmap
 * drawn at fit size, which is exactly the blur we must not show when checking focus.
 */
function showZoom() {
  const f = $("dframe"), img = $("dimg");
  const zoomed = zoom.s > 1.001;
  if (zoomed) {
    const w = Math.round(zoom.bw * zoom.s), h = Math.round(zoom.bh * zoom.s);
    f.style.cssText = `position:absolute;left:0;top:0;width:${w}px;height:${h}px;max-width:none;max-height:none;transform:translate(${Math.round(zoom.tx)}px,${Math.round(zoom.ty)}px)`;
    img.style.cssText = "width:100%;height:100%;max-width:none;max-height:none";
  } else {
    // Always the full fitted size, even while only the small thumbnail is loaded.
    const [w, h] = fitSize();
    f.style.cssText = `width:${w}px;height:${h}px;max-width:none;max-height:none`;
    img.style.cssText = "width:100%;height:100%;max-width:none;max-height:none";
  }
  f.style.setProperty("--zs", 1);
  $("dstage").classList.toggle("zoomed", zoomed);
  if (zoom.fullLoading) return;
  $("zPct").textContent = !zoomed ? "Vừa khung" : Math.round((zoom.s / scale100()) * 100) + "%";
}

/** Zoom to scale `s` keeping the screen point (cx, cy) fixed; null = centre of the view. */
function zoomTo(s, at) {
  const f = $("dframe"), stage = $("dstage").getBoundingClientRect();
  if (zoom.s <= 1.001) {
    // Leaving the fitted view: remember where and how big the image sits.
    const r = f.getBoundingClientRect();
    zoom.bw = r.width || 1; zoom.bh = r.height || 1;
    zoom.tx = r.left - stage.left; zoom.ty = r.top - stage.top;
  }
  s = Math.min(Math.max(s, 1), Z_MAX * Math.max(1, scale100()));
  if (s <= 1.001) {
    zoom.s = 1; zoom.tx = 0; zoom.ty = 0;
    return showZoom();
  }
  const px = at ? at[0] - stage.left : stage.width / 2, py = at ? at[1] - stage.top : stage.height / 2;
  const ux = (px - zoom.tx) / zoom.s, uy = (py - zoom.ty) / zoom.s; // point on the fitted image
  zoom.tx = px - ux * s;
  zoom.ty = py - uy * s;
  zoom.s = s;
  clampPan();
  showZoom();
  const dpr = window.devicePixelRatio || 1;
  if (!zoom.full && zoom.s * zoom.bw * dpr > $("dimg").naturalWidth * 0.95) ensureFull();
}

/** Keep at least part of the image on screen (coordinates inside the stage). */
function clampPan() {
  const stage = $("dstage").getBoundingClientRect();
  const w = zoom.bw * zoom.s, h = zoom.bh * zoom.s;
  const minX = stage.width - 80 - w, maxX = 80;
  const minY = stage.height - 80 - h, maxY = 80;
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
  applyVerdicts(await invoke("set_decision", { id: st.sid, idx: i, decision }));
}

/** One row per main face: Sơn tells the AI whether the eyes are open or closed. */
function renderEyes(i, p, mains) {
  const box = $("deyes");
  const rows = mains.filter((x) => x.f.eyes).slice(0, 8);
  box.hidden = !rows.length || !!p.error;
  if (box.hidden) { box.innerHTML = ""; return; }
  box.innerHTML = `<div class="h"><span>Mắt</span><span class="note">AI nhìn sai? Bấm để sửa, AI nhớ cho lần sau</span></div>` + rows
    .map(({ f, n }, k) => `<div class="eyerow" data-n="${n}">
      <span>${rows.length > 1 || mains.length > 1 ? `Người ${mains.findIndex((x) => x.n === n) + 1}` : "Mắt"}${f.by_hand ? ' <em class="note">(bạn sửa)</em>' : ""}</span>
      <button class="btn sm${!f.closed ? " on" : ""}" data-open="1">Mở</button>
      <button class="btn sm${f.closed ? " on" : ""}" data-open="0">Nhắm</button>
      ${f.by_hand ? `<button class="btn sm" data-open="" title="Để AI tự xét lại mặt này">↺</button>` : ""}
    </div>`)
    .join("");
  for (const row of box.querySelectorAll(".eyerow")) {
    const n = +row.dataset.n;
    const fb = () => $("dframe").querySelector(`.fbox[data-n="${n}"]`);
    row.onmouseenter = () => fb()?.classList.add("hl");
    row.onmouseleave = () => fb()?.classList.remove("hl");
    for (const b of row.querySelectorAll("button")) b.onclick = () => setEyes(i, n, b.dataset.open === "" ? null : b.dataset.open === "1");
  }
}

async function setEyes(i, face, open) {
  let r;
  try {
    r = await invoke("set_eyes", { id: st.sid, idx: i, face, open });
  } catch (e) {
    toast(esc(String(e)));
    return;
  }
  st.session.photos[i] = r.photo;
  applyVerdicts({ verdicts: r.verdicts, summary: r.summary });
  toast(open == null ? "Đã để AI tự xét lại mắt người này." : `Đã ghi nhớ: mắt ${open ? "mở" : "nhắm"}. AI đã học từ ${r.corrected} lần bạn sửa mắt và xét lại các ảnh khác.`, 3500);
  // Other photos may have changed too (the eye rule learned): refresh their notes quietly.
  const sid = st.sid;
  invoke("get_session", { id: sid }).then((v) => {
    if (st.sid !== sid || !st.session) return;
    st.session.photos = v.photos;
    if (st.views[sid]) st.views[sid].photos = v.photos;
  }).catch(() => {});
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

// ---------------------------------------------------------------- chat with the AI

// Quick requests, grouped. Each one is just text the chat understands; typing works the same.
const CHAT_GROUPS = [
  ["Loại sự kiện", ["Đây là đám cưới", "Hội nghị – hội thảo", "Sân khấu – biểu diễn", "Tiệc – gala", "Kỷ yếu – ảnh nhóm", "Thể thao"]],
  ["Người & khuôn mặt", ["Mắt nhân vật chính phải mở", "Loại ảnh há miệng, khẩu hình xấu", "Mặt nghiêng đẹp vẫn giữ", "Ưu tiên nhân vật chính, cận mặt", "Ưu tiên ảnh có người cười", "Giữ ảnh nhắm mắt"]],
  ["Bố cục & kỹ thuật", ["Bố cục, góc chụp quan trọng nhất", "Độ nét quan trọng nhất", "Tôi thích ảnh tông tối", "Không quan tâm màu sắc", "Bỏ qua nhiễu hạt", "Sửa thay vì loại"]],
  ["Số lượng", ["Chỉ giữ 300 ảnh", "Chọn nhiều hơn", "Khắt khe hơn", "Mỗi loạt giữ 2 ảnh", "Đủ khoảnh khắc", "Bỏ giới hạn số ảnh"]],
  ["Gu của tôi", ["Chọn theo phong cách của tôi", "Tin gu của tôi nhiều hơn", "AI đã học được gì từ tôi?", "Chỉ dùng tiêu chí máy, tắt gu"]],
  ["Hỏi & hoàn tác", ["Tóm tắt phiên này", "Vì sao nhiều ảnh bị loại?", "Đang chấm theo thiết lập nào?", "Hoàn tác", "Đặt lại mặc định"]],
];
let chatGroup = 0;
let chatBusy = false;
/** Settings before each chat change, newest last, so «Hoàn tác» can step back. */
const chatUndo = [];
const isUndo = (t) => /^(hoàn tác|hoan tac|undo|quay lại như cũ|quay lai nhu cu|trả lại như cũ|tra lai nhu cu)/i.test(t.trim());

function renderChips() {
  $("chatTabs").innerHTML = CHAT_GROUPS.map(([g], i) => `<button type="button" class="${i === chatGroup ? "on" : ""}" data-i="${i}">${esc(g)}</button>`).join("");
  for (const b of $("chatTabs").children) b.onclick = () => { chatGroup = +b.dataset.i; renderChips(); };
  $("chatChips").innerHTML = CHAT_GROUPS[chatGroup][1].map((x) => `<button type="button">${esc(x)}</button>`).join("");
  for (const b of $("chatChips").children) b.onclick = () => sendChat(b.textContent);
}

function showPrefs(prefs) {
  st.prefs = prefs;
  const m = meta();
  if (m) m.prefs = JSON.parse(JSON.stringify(prefs));
  renderPresets();
  renderWeights();
  renderRules();
  renderOptions();
  renderAiSwitch();
}

async function undoChat() {
  const prev = chatUndo.pop();
  if (!prev) {
    chatAdd("ai", `Chưa có thay đổi nào từ trò chuyện để hoàn tác.<span class="by">AI trong máy</span>`);
    return;
  }
  const before = st.session && st.verdicts ? st.verdicts.filter((v) => v.decision.kind === "selected").length : null;
  const v = await invoke("save_prefs", { id: st.sid, prefs: prev });
  showPrefs(prev);
  if (v && st.session) applyVerdicts(v);
  const after = v ? v.verdicts.filter((x) => x.decision.kind === "selected").length : null;
  chatAdd("ai", `Đã trả thiết lập về như trước lần chỉnh vừa rồi.${before != null && after != null ? `\nẢnh đã chọn: ${before} → ${after}.` : ""}<span class="by">AI trong máy</span>`);
}

function chatAdd(kind, html) {
  const d = document.createElement("div");
  d.className = "msg " + kind;
  d.innerHTML = html;
  $("chatMsgs").appendChild(d);
  $("chatMsgs").scrollTop = 1e9;
  return d;
}

async function sendChat(text) {
  text = text.trim();
  if (!text || chatBusy) return;
  chatBusy = true;
  $("chatInput").value = "";
  chatAdd("me", esc(text));
  if (isUndo(text)) {
    try { await undoChat(); } catch (e) { chatAdd("ai", esc(e) + `<span class="by">lỗi</span>`); }
    chatBusy = false;
    return;
  }
  const before = JSON.parse(JSON.stringify(st.prefs));
  const claude = $("chatClaude").checked;
  const wait = chatAdd("ai wait", claude ? "Đang hỏi Claude…" : "Đang xử lý…");
  try {
    const r = await invoke("chat", { id: st.sid, message: text, useClaude: claude });
    wait.remove();
    chatAdd("ai", esc(r.reply) + (r.changes.length ? `<ul>${r.changes.map((c) => `<li>${esc(c)}</li>`).join("")}</ul>` : "") + `<span class="by">${r.by === "claude" ? "Claude" : "AI trong máy"}</span>`);
    if (r.prefs) {
      chatUndo.push(before);
      if (chatUndo.length > 20) chatUndo.shift();
      showPrefs(r.prefs);
    }
    if (r.verdicts && st.session) applyVerdicts(r.verdicts);
  } catch (e) {
    wait.remove();
    chatAdd("ai", esc(e) + `<span class="by">lỗi</span>`);
  } finally {
    chatBusy = false;
  }
}

function wireChat() {
  const open = (on) => {
    $("chat").hidden = !on;
    $("chatFab").hidden = on;
    $("chatFab").setAttribute("aria-expanded", String(on));
    if (on) $("chatInput").focus();
  };
  $("chatFab").onclick = () => open(true);
  $("chatClose").onclick = () => open(false);
  renderChips();
  $("chatForm").onsubmit = (e) => { e.preventDefault(); sendChat($("chatInput").value); };
  $("chatInput").onkeydown = (e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); sendChat($("chatInput").value); } };
  $("chatClaude").onchange = () => {
    $("chatWho").textContent = $("chatClaude").checked ? "Đang dùng Claude qua Claude Code" : "Đang dùng AI trong máy";
    try { localStorage.setItem("useClaude", $("chatClaude").checked ? "1" : "0"); } catch (e) { /* storage off */ }
  };
  chatAdd("ai", "Bạn muốn chọn ảnh theo kiểu nào? Nói tự nhiên, ví dụ «ưu tiên ảnh có người cười, chỉ giữ 300 ảnh». Tôi chỉnh cách chấm và chọn lại ngay cho phiên đang mở.\n\nAI cũng học gu của bạn mỗi lần bạn xuất ảnh hoặc sửa tay.<span class=\"by\">AI trong máy</span>");
  checkClaude(false);
}

/** Look for Claude Code on this computer; `asked` = the user pressed «Kiểm tra lại». */
async function checkClaude(asked) {
  const hint = $("chatHint");
  if (asked) hint.textContent = "Đang tìm Claude Code trên máy…";
  let c;
  try { c = await invoke("claude_status"); } catch (e) { c = { found: false, hint: String(e) }; }
  $("chatClaude").disabled = !c.found;
  if (c.found) {
    hint.textContent = `${c.hint} · ${c.version || ""}`;
    let pref = asked ? "1" : "0";
    try { pref = asked ? "1" : localStorage.getItem("useClaude") || "0"; } catch (e) { /* storage off */ }
    if (pref === "1") { $("chatClaude").checked = true; $("chatClaude").onchange(); }
  } else {
    hint.innerHTML = `${esc(c.hint)} <button type="button" class="btn sm" id="chatRecheck">Kiểm tra lại</button>`;
    $("chatRecheck").onclick = () => checkClaude(true);
  }
}

// ---------------------------------------------------------------- web version

function renderWeb(w) {
  const box = $("webBox");
  if (window.RP_WEB) {
    box.innerHTML = `<p class="note">Bạn đang dùng bản web. Thư mục, ảnh và AI đã học đều là của máy tính đang mở SonVu Raw Pick; chọn hay chuyển ảnh ở đây là làm trên máy đó.</p>
      <button class="btn" id="webLogout">Đăng xuất</button>`;
    $("webLogout").onclick = async () => { await fetch("/api/logout", { method: "POST", headers: { "X-RP": "1" } }); location.reload(); };
    return;
  }
  if (!w) return;
  const r = w.remote;
  const remoteLine = !r.on ? "" : r.state === "ready"
    ? `<div class="wurl"><a href="#" data-url="${esc(r.url)}">${esc(r.url)}</a><button class="btn sm" data-copy="${esc(r.url)}">Chép</button></div>`
    : r.state === "error" ? `<p class="note err">${esc(r.error || "Không kết nối được")}</p>`
    : `<p class="note">${esc(r.error || "Đang mở đường truy cập từ xa…")}</p>`;
  box.innerHTML = `
    <div class="wurl"><a href="#" data-url="${esc(w.local_url)}">${esc(w.local_url)}</a></div>
    <label class="chk"><input type="checkbox" id="webLan" ${w.lan ? "checked" : ""}> Cho điện thoại, máy khác cùng wifi vào</label>
    ${w.lan && w.lan_url ? `<div class="wurl"><a href="#" data-url="${esc(w.lan_url)}">${esc(w.lan_url)}</a><button class="btn sm" data-copy="${esc(w.lan_url)}">Chép</button></div>` : ""}
    <label class="chk"><input type="checkbox" id="webRemote" ${r.on ? "checked" : ""}> Vào từ bất kỳ đâu qua internet</label>
    ${remoteLine}
    <div class="wpass">Mật khẩu <code>${esc(w.password)}</code><button class="btn sm" id="webNewPass">Đổi</button></div>
    <details class="wfixed" ${w.fixed_host ? "open" : ""}>
      <summary>Địa chỉ cố định (tên miền riêng)</summary>
      <p class="note">Link internet tự sinh sẽ đổi mỗi lần mở ứng dụng. Muốn địa chỉ cố định (ví dụ pick.sonvumedia.vn), tạo một Tunnel trong tài khoản Cloudflare miễn phí rồi dán mã vào đây.</p>
      <input id="webHost" placeholder="pick.tenmien.vn" value="${esc(w.fixed_host || "")}">
      <input id="webToken" type="password" placeholder="${w.has_token ? "Đã lưu mã tunnel (dán mã mới để thay)" : "Mã tunnel Cloudflare"}">
      <button class="btn sm" id="webSaveFixed">Lưu</button>
    </details>
    <p class="note">Ai có mật khẩu là điều khiển được máy này: mở thư mục, chọn, chuyển và xuất ảnh. Máy phải bật và đang mở ứng dụng thì web mới vào được.</p>`;
  for (const a of box.querySelectorAll("a[data-url]")) a.onclick = (e) => { e.preventDefault(); invoke("open_web", { path: a.dataset.url }).catch((x) => toast(esc(String(x)))); };
  for (const b of box.querySelectorAll("[data-copy]")) b.onclick = () => navigator.clipboard.writeText(b.dataset.copy).then(() => toast("Đã chép link."), () => toast(esc(b.dataset.copy)));
  const set = (args) => invoke("web_set", args).then(renderWeb).catch((x) => toast(esc(String(x))));
  $("webLan").onchange = () => set({ lan: $("webLan").checked });
  $("webRemote").onchange = () => set({ remote: $("webRemote").checked });
  $("webNewPass").onclick = async () => {
    if (await ask("Mọi trình duyệt đang vào bản web sẽ phải đăng nhập lại.", { title: "Tạo mật khẩu mới?", ok: "Tạo mật khẩu mới" })) set({ newPassword: true });
  };
  $("webSaveFixed").onclick = () => set({ tunnelHost: $("webHost").value.trim(), tunnelToken: $("webToken").value.trim() || null });
}

function wireWeb() {
  if (window.RP_WEB) return renderWeb(null);
  invoke("web_info").then(renderWeb).catch(() => { $("webSec").hidden = true; });
  T.event.listen("web-changed", (e) => renderWeb(e.payload));
}

// ---------------------------------------------------------------- boot

(async function boot() {
  st.info = await invoke("app_info");
  st.prefs = JSON.parse(JSON.stringify(st.info.defaults));
  st.metas = st.info.sessions;
  renderMeta();
  wireRail();
  wireIn();
  wireResults();
  wireDetail();
  wireZoom();
  wireChat();
  wireWeb();
  renderDrive();
  renderSync();
  if (!st.metas.length) await newSession();
  await selectSession(st.metas[0].id);
  restoreSessions();
})();
