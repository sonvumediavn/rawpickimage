// Web version: the same page as the desktop window, talking to SonVu Raw Pick on the computer
// over HTTP. Loaded only by the web version (the app inserts it before app.js), never by the window.
(() => {
  "use strict";
  window.RP_WEB = true;
  document.documentElement.classList.add("web");
  const demo = window.RP_DEMO_API; // preview page with sample photos, no computer behind it

  let ready;
  const whenReady = new Promise((r) => (ready = r));
  const listeners = {};

  const el = (tag, attrs = {}, ...kids) => {
    const e = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) {
      if (k === "class") e.className = v;
      else if (k.startsWith("on")) e[k] = v;
      else if (v !== false && v != null) e.setAttribute(k, v === true ? "" : v);
    }
    for (const c of kids.flat()) if (c != null) e.append(c);
    return e;
  };

  async function api(cmd, args) {
    if (demo) return demo(cmd, args || {});
    await whenReady;
    const r = await fetch("/api/" + cmd, {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "application/json", "X-RP": "1" },
      body: JSON.stringify(args || {}),
    });
    if (r.status === 401) {
      showLogin("Phiên đăng nhập đã hết, nhập lại mật khẩu.");
      throw "Cần đăng nhập lại";
    }
    const text = await r.text();
    let data;
    try { data = text ? JSON.parse(text) : null; } catch { throw text || "Lỗi " + r.status; }
    if (!r.ok) throw (data && data.error) || "Lỗi " + r.status;
    return data;
  }

  function connectEvents() {
    if (demo) return;
    const es = new EventSource("/api/events");
    es.onmessage = (m) => {
      let d;
      try { d = JSON.parse(m.data); } catch { return; }
      for (const cb of listeners[d.event] || []) cb({ event: d.event, payload: d.payload });
    };
  }

  // ------------------------------------------------------------- login
  let loginBox = null;
  function showLogin(msg) {
    if (loginBox) return;
    const err = el("p", { class: "note err" }, msg || "");
    const input = el("input", { type: "password", autocomplete: "current-password", placeholder: "Mật khẩu hoặc mã PIN", "aria-label": "Mật khẩu" });
    const go = el("button", { class: "btn primary", type: "submit" }, "Vào");
    const form = el("form", { class: "box login" },
      el("h3", {}, "SONVU RAW PICK"),
      el("p", { class: "note" }, "Nhập mật khẩu bản web (xem trong ứng dụng trên máy tính, mục Bản web)."),
      input, go, err);
    form.onsubmit = async (e) => {
      e.preventDefault();
      go.disabled = true;
      err.textContent = "";
      try {
        const r = await fetch("/api/login", { method: "POST", credentials: "same-origin", headers: { "Content-Type": "application/json", "X-RP": "1" }, body: JSON.stringify({ password: input.value }) });
        const d = await r.json().catch(() => ({}));
        if (!r.ok) throw d.error || "Sai mật khẩu";
        location.reload();
      } catch (x) {
        err.textContent = String(x);
        go.disabled = false;
        input.select();
      }
    };
    loginBox = el("div", { class: "modal" }, form);
    document.body.append(loginBox);
    input.focus();
  }

  // ------------------------------------------------------------- folder picker
  /** Browse folders of the computer running the app (a browser can't see them by itself). */
  function pickFolder(opts = {}) {
    return new Promise((resolve) => {
      const title = el("h3", {}, opts.title || "Chọn thư mục");
      const pathIn = el("input", { class: "fpath", spellcheck: "false", "aria-label": "Đường dẫn" });
      const up = el("button", { class: "btn sm", type: "button", title: "Lên thư mục cha" }, "↑");
      const places = el("div", { class: "fplaces" });
      const list = el("ul", { class: "flist", role: "listbox" });
      const info = el("span", { class: "meta" });
      const cancel = el("button", { class: "btn", type: "button" }, "Huỷ");
      const choose = el("button", { class: "btn primary", type: "button" }, "Chọn thư mục này");
      const box = el("div", { class: "box fpick" }, title,
        el("div", { class: "frow" }, up, pathIn), places, list,
        el("div", { class: "frow end" }, info, cancel, choose));
      const wrap = el("div", { class: "modal" }, box);
      let cur = null;
      const close = (v) => { wrap.remove(); document.removeEventListener("keydown", onKey); resolve(v); };
      const onKey = (e) => { if (e.key === "Escape") close(null); };
      async function go(path) {
        list.classList.add("loading");
        try {
          const d = await api("list_dir", { path: path || null });
          cur = d;
          pathIn.value = d.path;
          up.disabled = !d.parent;
          list.replaceChildren(...d.dirs.map((x) => el("li", { role: "option", tabindex: "0", onclick: () => go(x.path), onkeydown: (e) => e.key === "Enter" && go(x.path) }, x.name)));
          if (!d.dirs.length) list.append(el("li", { class: "none" }, "Không có thư mục con"));
          info.textContent = d.photos ? `${d.photos} file ảnh trong thư mục này` : "";
          if (!places.childElementCount) places.replaceChildren(...d.places.map((p) => el("button", { class: "btn sm", type: "button", onclick: () => go(p.path) }, p.name)));
        } catch (e) {
          info.textContent = String(e);
        } finally {
          list.classList.remove("loading");
        }
      }
      up.onclick = () => cur && cur.parent && go(cur.parent);
      pathIn.onkeydown = (e) => { if (e.key === "Enter") go(pathIn.value.trim()); };
      cancel.onclick = () => close(null);
      choose.onclick = () => close(cur ? cur.path : null);
      wrap.onclick = (e) => { if (e.target === wrap) close(null); };
      document.addEventListener("keydown", onKey);
      document.body.append(wrap);
      go(opts.defaultPath);
    });
  }

  // ------------------------------------------------------------- the Tauri API the page uses
  window.__TAURI__ = {
    core: {
      invoke: (cmd, args) => (cmd === "call" ? api(args.cmd, args.args) : api(cmd, args)),
      convertFileSrc: (p) => (demo ? p : "/file?p=" + encodeURIComponent(p)),
    },
    event: {
      listen: async (name, cb) => {
        (listeners[name] ||= []).push(cb);
        return () => { listeners[name] = listeners[name].filter((x) => x !== cb); };
      },
    },
    webview: { getCurrentWebview: () => ({ onDragDropEvent: async () => () => {} }) },
    dialog: { open: (opts) => pickFolder(opts) },
    opener: {
      openPath: async (path) => {
        await api("open_path", { path });
        if (typeof window.toast === "function") window.toast("Đã mở thư mục trên máy tính chạy ứng dụng.");
      },
    },
  };

  // A browser can't hand over dropped folders, so the drop zone becomes a button.
  const drop = document.getElementById("drop");
  if (drop) {
    drop.querySelector("strong").textContent = "Bấm để chọn thư mục ảnh";
    drop.querySelector("small").textContent = "thư mục trên máy tính chạy SonVu Raw Pick · mỗi thư mục một phiên · RAW và JPEG";
  }
  const big = document.querySelector("#empty .big");
  if (big) big.textContent = "Chọn thư mục ảnh sự kiện trên máy tính.";

  if (demo) {
    ready();
  } else {
    fetch("/api/whoami", { credentials: "same-origin" })
      .then((r) => r.json())
      .then((d) => {
        if (d.auth) {
          ready();
          connectEvents();
        } else showLogin();
      })
      .catch(() => showLogin("Không kết nối được máy tính. Kiểm tra máy đã bật và đang mở SonVu Raw Pick."));
  }
})();
