//! The web version. The app serves its own page over HTTP so a browser can drive it with the same
//! powers as the window: on this computer, on other devices in the same network when allowed,
//! and from anywhere through a Cloudflare tunnel. Photos never leave the computer; the browser
//! only gets thumbnails and previews. Every request except the login page needs the password.

use crate::{dispatch, App};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, Server};

const INDEX: &str = include_str!("../../ui/index.html");
const STYLE: &str = include_str!("../../ui/style.css");
const APP_JS: &str = include_str!("../../ui/app.js");
const WEB_JS: &str = include_str!("../../ui/web.js");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebPrefs {
    /// Let phones and other computers in the same network in (listen on all interfaces).
    pub lan: bool,
    pub port: u16,
    pub password: String,
    /// Reachable from anywhere through a Cloudflare tunnel.
    pub remote: bool,
    /// Fixed address: a tunnel made in Sơn's Cloudflare account (token) and its hostname.
    pub tunnel_token: String,
    pub tunnel_host: String,
}

impl Default for WebPrefs {
    fn default() -> Self {
        Self { lan: false, port: 8686, password: String::new(), remote: false, tunnel_token: String::new(), tunnel_host: String::new() }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RemoteView {
    on: bool,
    /// off | starting | ready | error
    state: &'static str,
    url: Option<String>,
    error: Option<String>,
}

#[derive(Default)]
pub struct Web {
    sse: Mutex<Vec<Sender<String>>>,
    tokens: Mutex<HashSet<String>>,
    /// One-time keys the window uses to open the web version already signed in.
    once: Mutex<HashMap<String, Instant>>,
    fails: Mutex<HashMap<String, (u32, Instant)>>,
    all_fails: Mutex<Vec<Instant>>,
    server: Mutex<Option<(Arc<Server>, u16)>>,
    error: Mutex<Option<String>>,
    tunnel: Mutex<Option<std::process::Child>>,
    tunnel_gen: AtomicU64,
    remote: Mutex<RemoteView>,
}

impl Web {
    pub fn broadcast<S: Serialize>(&self, event: &str, payload: &S) {
        let mut v = self.sse.lock().unwrap();
        if v.is_empty() {
            return;
        }
        let msg = serde_json::json!({ "event": event, "payload": payload }).to_string();
        v.retain(|tx| tx.send(msg.clone()).is_ok());
    }

    /// Stop the tunnel (on exit, or before starting another one).
    pub fn stop_tunnel(&self) {
        self.tunnel_gen.fetch_add(1, Ordering::SeqCst);
        if let Some(mut c) = self.tunnel.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    if getrandom::fill(&mut b).is_err() {
        // Should never happen; fall back to the OS-seeded hasher keys.
        use std::hash::{BuildHasher, Hasher};
        for (i, x) in b.iter_mut().enumerate() {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_usize(i);
            *x = h.finish() as u8;
        }
    }
    b
}

fn random_token() -> String {
    random_bytes::<24>().iter().map(|b| format!("{b:02x}")).collect()
}

/// Easy to read out loud and type on a phone: 10 characters, no look-alikes (≈ 50 bits).
fn random_password() -> String {
    const A: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let b = random_bytes::<10>();
    let s: String = b.iter().map(|x| A[*x as usize % A.len()] as char).collect();
    format!("{}-{}", &s[..5], &s[5..])
}

// ---------------------------------------------------------------- server life

/// Start (or restart) the web server with the saved settings. Always on for this computer;
/// other devices only when Sơn allows it.
pub fn start(app: &Arc<App>) {
    let prefs = {
        let mut g = app.global.lock().unwrap();
        if g.web.password.len() < 8 {
            g.web.password = random_password();
            drop(g);
            app.save_global();
            app.global.lock().unwrap().web.clone()
        } else {
            g.web.clone()
        }
    };
    if let Some((s, _)) = app.web.server.lock().unwrap().take() {
        s.unblock();
    }
    let host = if prefs.lan { "0.0.0.0" } else { "127.0.0.1" };
    let mut err = None;
    for port in prefs.port..prefs.port.saturating_add(20) {
        match Server::http((host, port)) {
            Ok(s) => {
                let s = Arc::new(s);
                *app.web.server.lock().unwrap() = Some((s.clone(), port));
                *app.web.error.lock().unwrap() = None;
                let a = app.clone();
                std::thread::spawn(move || {
                    for req in s.incoming_requests() {
                        let a = a.clone();
                        std::thread::spawn(move || handle(&a, req));
                    }
                });
                if prefs.remote {
                    start_tunnel(app);
                }
                return;
            }
            Err(e) => err = Some(e.to_string()),
        }
    }
    *app.web.error.lock().unwrap() = err.map(|e| format!("Không mở được bản web: {e}"));
}

fn port(app: &App) -> Option<u16> {
    app.web.server.lock().unwrap().as_ref().map(|(_, p)| *p)
}

/// This computer's address in the local network (no packet is sent).
fn lan_ip() -> Option<IpAddr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

#[derive(Serialize, Clone)]
pub struct WebInfo {
    running: bool,
    local_url: String,
    lan: bool,
    lan_url: Option<String>,
    password: String,
    remote: RemoteView,
    fixed_host: Option<String>,
    has_token: bool,
    error: Option<String>,
}

pub fn info(app: &App) -> WebInfo {
    let p = app.global.lock().unwrap().web.clone();
    let port = port(app);
    let pt = port.unwrap_or(p.port);
    WebInfo {
        running: port.is_some(),
        local_url: format!("http://127.0.0.1:{pt}"),
        lan: p.lan,
        lan_url: p.lan.then(lan_ip).flatten().map(|ip| format!("http://{ip}:{pt}")),
        password: p.password,
        remote: app.web.remote.lock().unwrap().clone(),
        fixed_host: (!p.tunnel_host.is_empty()).then_some(p.tunnel_host),
        has_token: !p.tunnel_token.is_empty(),
        error: app.web.error.lock().unwrap().clone(),
    }
}

/// Change the web settings from the window.
pub fn set(app: &Arc<App>, lan: Option<bool>, remote: Option<bool>, new_password: bool, host: Option<String>, token: Option<String>) -> Result<WebInfo, String> {
    let (restart_server, restart_tunnel, remote_on) = {
        let mut g = app.global.lock().unwrap();
        let w = &mut g.web;
        let mut rs = false;
        let mut rt = false;
        if let Some(l) = lan {
            rs |= w.lan != l;
            w.lan = l;
        }
        if let Some(r) = remote {
            rt |= w.remote != r;
            w.remote = r;
        }
        if let Some(h) = host {
            let h = h.trim().trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_lowercase();
            if !h.is_empty() && (!h.contains('.') || h.contains(['/', ' ', ':'])) {
                return Err("Tên miền không hợp lệ, ví dụ đúng: pick.tenmien.vn".into());
            }
            rt |= w.tunnel_host != h;
            w.tunnel_host = h;
        }
        if let Some(t) = token {
            // Cloudflare shows the token inside a command; keep only the token itself.
            let t = t.split_whitespace().last().unwrap_or("").to_string();
            rt |= w.tunnel_token != t;
            w.tunnel_token = t;
        }
        if new_password {
            w.password = random_password();
        }
        (rs, rt, w.remote)
    };
    app.save_global();
    if new_password {
        app.web.tokens.lock().unwrap().clear();
    }
    if restart_server {
        start(app); // restarts the tunnel too when it is on
    } else if restart_tunnel {
        if remote_on {
            start_tunnel(app);
        } else {
            app.web.stop_tunnel();
            *app.web.remote.lock().unwrap() = RemoteView { state: "off", ..Default::default() };
        }
    }
    Ok(info(app))
}

/// Open the web version in this computer's browser, already signed in.
pub fn open(app: &App, url: &str) -> Result<(), String> {
    let key = random_token();
    let mut once = app.web.once.lock().unwrap();
    once.retain(|_, t| t.elapsed() < Duration::from_secs(120));
    once.insert(key.clone(), Instant::now());
    drop(once);
    let url = format!("{}/?k={key}", url.trim_end_matches('/'));
    tauri_plugin_opener::open_url(&url, None::<&str>).map_err(|e| format!("Không mở được trình duyệt: {e}"))
}

// ---------------------------------------------------------------- tunnel (Cloudflare)

fn tunnel_state(app: &App, state: &'static str, url: Option<String>, error: Option<String>) {
    *app.web.remote.lock().unwrap() = RemoteView { on: true, state, url, error };
    app.emit("web-changed", info(app));
}

#[allow(unused_mut)]
fn hidden(mut c: std::process::Command) -> std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// Our own copy of cloudflared, downloaded once from Cloudflare's GitHub releases.
fn cloudflared(app: &App) -> Result<PathBuf, String> {
    let dir = app.data_dir.join("bin");
    let exe = dir.join(if cfg!(windows) { "cloudflared.exe" } else { "cloudflared" });
    if exe.exists() {
        return Ok(exe);
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "amd64" };
    let base = "https://github.com/cloudflare/cloudflared/releases/latest/download";
    let (file, archive) = if cfg!(windows) {
        ("cloudflared-windows-amd64.exe".to_string(), false)
    } else if cfg!(target_os = "macos") {
        (format!("cloudflared-darwin-{arch}.tgz"), true)
    } else {
        (format!("cloudflared-linux-{arch}"), false)
    };
    let tmp = dir.join(format!("{file}.part"));
    let curl = if cfg!(windows) { "curl.exe" } else { "curl" };
    let mut c = hidden(std::process::Command::new(curl));
    c.args(["-fsSL", "--retry", "2", "-o"]).arg(&tmp).arg(format!("{base}/{file}"));
    let out = c.output().map_err(|e| format!("Không tải được công cụ kết nối (curl): {e}"))?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("Không tải được công cụ kết nối Cloudflare: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    if archive {
        let mut t = std::process::Command::new("tar");
        t.arg("-xzf").arg(&tmp).arg("-C").arg(&dir);
        let ok = t.status().map(|s| s.success()).unwrap_or(false);
        let _ = std::fs::remove_file(&tmp);
        if !ok || !exe.exists() {
            return Err("Không giải nén được công cụ kết nối Cloudflare".into());
        }
    } else {
        std::fs::rename(&tmp, &exe).map_err(|e| e.to_string())?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755));
    }
    Ok(exe)
}

fn find_quick_url(line: &str) -> Option<String> {
    let i = line.find("https://")?;
    let rest = &line[i..];
    let end = rest.find(|c: char| c.is_whitespace() || c == '|').unwrap_or(rest.len());
    let url = &rest[..end];
    (url.ends_with(".trycloudflare.com") && url != "https://api.trycloudflare.com").then(|| url.to_string())
}

/// Start the tunnel in the background and keep it up (restarts after a drop).
pub fn start_tunnel(app: &Arc<App>) {
    app.web.stop_tunnel();
    let gen = app.web.tunnel_gen.load(Ordering::SeqCst);
    tunnel_state(app, "starting", None, Some("Đang mở đường truy cập từ xa…".into()));
    let app = app.clone();
    std::thread::spawn(move || {
        let mut tries = 0u32;
        loop {
            if app.web.tunnel_gen.load(Ordering::SeqCst) != gen {
                return;
            }
            let Some(port) = port(&app) else {
                tunnel_state(&app, "error", None, Some("Bản web chưa chạy trên máy này".into()));
                return;
            };
            let exe = match cloudflared(&app) {
                Ok(e) => e,
                Err(e) => {
                    tunnel_state(&app, "error", None, Some(e));
                    return;
                }
            };
            let w = app.global.lock().unwrap().web.clone();
            let fixed = !w.tunnel_token.is_empty() && !w.tunnel_host.is_empty();
            let mut c = hidden(std::process::Command::new(&exe));
            c.args(["tunnel", "--no-autoupdate"]);
            if fixed {
                // The token goes through the environment so it doesn't show in the process list.
                c.arg("run").env("TUNNEL_TOKEN", &w.tunnel_token);
            } else {
                c.arg("--url").arg(format!("http://127.0.0.1:{port}"));
            }
            c.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
            let mut child = match c.spawn() {
                Ok(ch) => ch,
                Err(e) => {
                    tunnel_state(&app, "error", None, Some(format!("Không chạy được công cụ kết nối: {e}")));
                    return;
                }
            };
            let stderr = child.stderr.take();
            *app.web.tunnel.lock().unwrap() = Some(child);
            let started = Instant::now();
            let mut last = String::new();
            if let Some(err) = stderr {
                for line in BufReader::new(err).lines().map_while(Result::ok) {
                    if app.web.tunnel_gen.load(Ordering::SeqCst) != gen {
                        return;
                    }
                    if fixed {
                        if line.contains("Registered tunnel connection") && app.web.remote.lock().unwrap().state != "ready" {
                            tries = 0;
                            tunnel_state(&app, "ready", Some(format!("https://{}", w.tunnel_host)), None);
                        }
                    } else if let Some(url) = find_quick_url(&line) {
                        tries = 0;
                        tunnel_state(&app, "ready", Some(url), None);
                    }
                    if line.contains(" ERR ") {
                        last = line.split(" ERR ").nth(1).unwrap_or(&line).chars().take(200).collect();
                    }
                }
            }
            // cloudflared exited.
            if let Some(mut ch) = app.web.tunnel.lock().unwrap().take() {
                let _ = ch.wait();
            }
            if app.web.tunnel_gen.load(Ordering::SeqCst) != gen {
                return;
            }
            tries += 1;
            if started.elapsed() > Duration::from_secs(120) {
                tries = 1;
            }
            if tries > 5 {
                let msg = if fixed { "Không kết nối được tunnel Cloudflare, kiểm tra lại mã tunnel" } else { "Không kết nối được Cloudflare, kiểm tra mạng" };
                tunnel_state(&app, "error", None, Some(if last.is_empty() { msg.to_string() } else { format!("{msg}: {last}") }));
                return;
            }
            tunnel_state(&app, "starting", None, Some("Mất kết nối, đang nối lại…".into()));
            std::thread::sleep(Duration::from_secs(5 * tries as u64));
        }
    });
}

// ---------------------------------------------------------------- folders (for the web picker)

#[derive(Serialize)]
pub struct DirEntry {
    name: String,
    path: String,
}

#[derive(Serialize)]
pub struct DirList {
    path: String,
    parent: Option<String>,
    dirs: Vec<DirEntry>,
    photos: usize,
    places: Vec<DirEntry>,
}

fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

fn places() -> Vec<DirEntry> {
    let mut v = vec![];
    let mut add = |name: &str, p: PathBuf| {
        if p.is_dir() {
            v.push(DirEntry { name: name.into(), path: p.display().to_string() });
        }
    };
    if let Some(h) = home() {
        add("Thư mục người dùng", h.clone());
        add("Màn hình", h.join("Desktop"));
        add("Ảnh", h.join("Pictures"));
        add("Tải về", h.join("Downloads"));
    }
    #[cfg(windows)]
    for d in b'A'..=b'Z' {
        let p = PathBuf::from(format!("{}:\\", d as char));
        if p.exists() {
            add(&format!("Ổ {}:", d as char), p);
        }
    }
    #[cfg(not(windows))]
    for root in ["/Volumes", "/media", "/mnt"] {
        if let Ok(rd) = std::fs::read_dir(root) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if !n.starts_with('.') {
                    add(&n, e.path());
                }
            }
        }
    }
    v
}

pub fn list_dir(path: Option<&str>) -> Result<DirList, String> {
    let p = match path.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => PathBuf::from(s),
        None => home().ok_or("Không tìm thấy thư mục người dùng")?,
    };
    let rd = std::fs::read_dir(&p).map_err(|e| format!("Không mở được {}: {e}", p.display()))?;
    let mut dirs = vec![];
    let mut photos = 0;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let path = e.path();
        if path.is_dir() {
            let skip = name.starts_with('.') || name.starts_with('$') || name == "System Volume Information";
            if !skip {
                dirs.push(DirEntry { name, path: path.display().to_string() });
            }
        } else if rawpick_core::preview::is_supported(&path) {
            photos += 1;
        }
    }
    dirs.sort_by_key(|d| d.name.to_lowercase());
    Ok(DirList { path: p.display().to_string(), parent: p.parent().map(|x| x.display().to_string()), dirs, photos, places: places() })
}

// ---------------------------------------------------------------- HTTP

fn header<'a>(req: &'a Request, name: &'static str) -> Option<&'a str> {
    req.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str())
}

fn hdr(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).expect("ascii header")
}

fn security_headers<R: Read>(r: Response<R>) -> Response<R> {
    r.with_header(hdr("X-Content-Type-Options", "nosniff"))
        .with_header(hdr("X-Frame-Options", "DENY"))
        .with_header(hdr("Referrer-Policy", "no-referrer"))
        .with_header(hdr("Cache-Control", "no-store"))
}

fn send_json<T: Serialize>(req: Request, code: u16, v: &T) {
    let body = serde_json::to_string(v).unwrap_or_else(|_| "null".into());
    let r = Response::from_string(body).with_status_code(code).with_header(hdr("Content-Type", "application/json; charset=utf-8"));
    let _ = req.respond(security_headers(r));
}

fn send_err(req: Request, code: u16, msg: &str) {
    send_json(req, code, &serde_json::json!({ "error": msg }));
}

fn percent_decode(s: &str) -> String {
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => match (hex(b[i + 1]), hex(b[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query(url: &str, key: &str) -> Option<String> {
    let q = url.split_once('?')?.1;
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (k == key).then(|| percent_decode(v))
    })
}

fn cookie_token(req: &Request) -> Option<String> {
    header(req, "Cookie")?.split(';').find_map(|c| c.trim().strip_prefix("rp=").map(str::to_string))
}

fn authed(app: &App, req: &Request) -> bool {
    cookie_token(req).is_some_and(|t| app.web.tokens.lock().unwrap().contains(&t))
}

/// Who is asking: through the tunnel every request comes from this computer, so use the
/// address Cloudflare reports.
fn client_id(req: &Request) -> String {
    let ip = req.remote_addr().map(|a| a.ip());
    match (ip, header(req, "Cf-Connecting-Ip")) {
        (Some(ip), Some(cf)) if ip.is_loopback() => cf.to_string(),
        (Some(ip), _) => ip.to_string(),
        _ => "?".into(),
    }
}

fn is_https(req: &Request) -> bool {
    header(req, "X-Forwarded-Proto").is_some_and(|p| p.eq_ignore_ascii_case("https"))
}

fn session_cookie(app: &App, req: &Request) -> Header {
    let t = random_token();
    {
        let mut tokens = app.web.tokens.lock().unwrap();
        if tokens.len() > 500 {
            tokens.clear();
        }
        tokens.insert(t.clone());
    }
    let secure = if is_https(req) { "; Secure" } else { "" };
    hdr("Set-Cookie", &format!("rp={t}; Path=/; HttpOnly; SameSite=Strict; Max-Age=2592000{secure}"))
}

/// Only addresses we know: blocks other websites reaching this server through DNS tricks.
fn host_ok(app: &App, req: &Request) -> bool {
    let Some(h) = header(req, "Host") else { return false };
    let name = if let Some(rest) = h.strip_prefix('[') { rest.split(']').next().unwrap_or("") } else { h.rsplit_once(':').map(|(n, _)| n).unwrap_or(h) };
    let name = name.to_ascii_lowercase();
    let w = app.global.lock().unwrap().web.clone();
    name == "localhost"
        || name.parse::<IpAddr>().is_ok()
        || name.ends_with(".local")
        || (w.remote && name.ends_with(".trycloudflare.com"))
        || (!w.tunnel_host.is_empty() && name == w.tunnel_host)
}

fn login_blocked(app: &App, who: &str) -> bool {
    let now = Instant::now();
    let mut all = app.web.all_fails.lock().unwrap();
    all.retain(|t| now.duration_since(*t) < Duration::from_secs(600));
    if all.len() >= 30 {
        return true;
    }
    let f = app.web.fails.lock().unwrap();
    matches!(f.get(who), Some((n, t)) if *n >= 5 && now.duration_since(*t) < Duration::from_secs(60))
}

fn login_failed(app: &App, who: &str) {
    let now = Instant::now();
    app.web.all_fails.lock().unwrap().push(now);
    let mut f = app.web.fails.lock().unwrap();
    let e = f.entry(who.to_string()).or_insert((0, now));
    if now.duration_since(e.1) > Duration::from_secs(60) {
        e.0 = 0;
    }
    e.0 += 1;
    e.1 = now;
}

fn handle(app: &Arc<App>, mut req: Request) {
    if !host_ok(app, &req) {
        return send_err(req, 403, "Địa chỉ không được phép");
    }
    let url = req.url().to_string();
    let path = url.split('?').next().unwrap_or("/").to_string();
    let method = req.method().clone();

    // The page itself (public: it only shows the login box until signed in).
    if method == Method::Get {
        let page = match path.as_str() {
            "/" | "/index.html" => Some((INDEX.replace("<script src=\"app.js\"></script>", "<script src=\"web.js\"></script>\n<script src=\"app.js\"></script>"), "text/html; charset=utf-8")),
            "/style.css" => Some((STYLE.to_string(), "text/css; charset=utf-8")),
            "/app.js" => Some((APP_JS.to_string(), "text/javascript; charset=utf-8")),
            "/web.js" => Some((WEB_JS.to_string(), "text/javascript; charset=utf-8")),
            _ => None,
        };
        if let Some((body, ty)) = page {
            let mut r = security_headers(Response::from_string(body).with_header(hdr("Content-Type", ty)));
            if ty.starts_with("text/html") {
                r = r.with_header(hdr("Content-Security-Policy", "default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; frame-ancestors 'none'"));
                // Opened from the window: sign in with the one-time key, then drop it from the address.
                if let Some(k) = query(&url, "k") {
                    let ok = app.web.once.lock().unwrap().remove(&k).is_some_and(|t| t.elapsed() < Duration::from_secs(120));
                    if ok {
                        let r = Response::empty(302).with_header(hdr("Location", "/")).with_header(session_cookie(app, &req));
                        let _ = req.respond(security_headers(r));
                        return;
                    }
                }
            }
            let _ = req.respond(r);
            return;
        }
        if path == "/favicon.ico" {
            let _ = req.respond(Response::empty(204));
            return;
        }
    }

    // Any API call that changes something must carry our header: browsers won't let another
    // website add it without asking this server first, which we never allow.
    if method == Method::Post && header(&req, "X-RP") != Some("1") {
        return send_err(req, 403, "Thiếu xác nhận");
    }

    match (method, path.as_str()) {
        (Method::Get, "/api/whoami") => {
            let a = authed(app, &req);
            send_json(req, 200, &serde_json::json!({ "auth": a }))
        }
        (Method::Post, "/api/login") => {
            let who = client_id(&req);
            if login_blocked(app, &who) {
                return send_err(req, 429, "Nhập sai nhiều lần, đợi vài phút rồi thử lại");
            }
            let mut body = String::new();
            let _ = req.as_reader().take(4096).read_to_string(&mut body);
            #[derive(Deserialize)]
            struct L {
                password: String,
            }
            let given = serde_json::from_str::<L>(&body).map(|l| l.password).unwrap_or_default();
            let want = app.global.lock().unwrap().web.password.clone();
            let norm = |s: &str| s.trim().to_lowercase().replace([' ', '-'], "");
            if !want.is_empty() && norm(&given) == norm(&want) {
                app.web.fails.lock().unwrap().remove(&who);
                let c = session_cookie(app, &req);
                let r = Response::from_string("{\"ok\":true}").with_header(hdr("Content-Type", "application/json")).with_header(c);
                let _ = req.respond(security_headers(r));
            } else {
                login_failed(app, &who);
                send_err(req, 401, "Sai mật khẩu")
            }
        }
        _ if !authed(app, &req) => send_err(req, 401, "Cần đăng nhập"),
        (Method::Post, "/api/logout") => {
            if let Some(t) = cookie_token(&req) {
                app.web.tokens.lock().unwrap().remove(&t);
            }
            send_json(req, 200, &serde_json::json!({ "ok": true }))
        }
        (Method::Get, "/api/events") => events(app, req),
        (Method::Get, "/file") => file(app, req, &url),
        (Method::Post, p) if p.starts_with("/api/") => {
            let cmd = p.trim_start_matches("/api/").to_string();
            // The web page can't change its own access (that stays in the window).
            if matches!(cmd.as_str(), "web_set" | "open_web") {
                return send_err(req, 403, "Chỉ chỉnh được trong ứng dụng trên máy tính");
            }
            let mut body = Vec::new();
            if req.as_reader().take(8 << 20).read_to_end(&mut body).is_err() {
                return send_err(req, 400, "Không đọc được yêu cầu");
            }
            let args: serde_json::Value = if body.is_empty() { serde_json::Value::Null } else { serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null) };
            match dispatch(app, &cmd, args) {
                Ok(v) => send_json(req, 200, &v),
                Err(e) => send_err(req, 400, &e),
            }
        }
        _ => send_err(req, 404, "Không có trang này"),
    }
}

/// Live progress for the page (Server-Sent Events).
fn events(app: &App, req: Request) {
    let (tx, rx) = channel::<String>();
    app.web.sse.lock().unwrap().push(tx);
    let mut w = req.into_writer();
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nX-Accel-Buffering: no\r\nConnection: keep-alive\r\n\r\nretry: 3000\n\n";
    if w.write_all(head.as_bytes()).and_then(|_| w.flush()).is_err() {
        return;
    }
    loop {
        let chunk = match rx.recv_timeout(Duration::from_secs(15)) {
            Ok(m) => format!("data: {m}\n\n"),
            Err(RecvTimeoutError::Timeout) => ": ping\n\n".to_string(),
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if w.write_all(chunk.as_bytes()).and_then(|_| w.flush()).is_err() {
            return;
        }
    }
}

/// Thumbnails and previews: only files inside the app's own data folder.
fn file(app: &App, req: Request, url: &str) {
    let Some(p) = query(url, "p") else { return send_err(req, 400, "Thiếu đường dẫn") };
    let base = app.data_dir.canonicalize().unwrap_or_else(|_| app.data_dir.clone());
    let ok = Path::new(&p).canonicalize().ok().filter(|c| c.starts_with(&base) && c.is_file());
    let Some(c) = ok else { return send_err(req, 404, "Không có ảnh này") };
    let ty = match c.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        _ => return send_err(req, 403, "Không được phép"),
    };
    match std::fs::File::open(&c) {
        Ok(f) => {
            let r = Response::from_file(f)
                .with_header(hdr("Content-Type", ty))
                .with_header(hdr("Cache-Control", "private, max-age=86400"))
                .with_header(hdr("X-Content-Type-Options", "nosniff"));
            let _ = req.respond(r);
        }
        Err(e) => send_err(req, 404, &e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_query_values() {
        assert_eq!(query("/file?p=C%3A%5CAnh%5Ca+b.jpg", "p").as_deref(), Some("C:\\Anh\\a b.jpg"));
        assert_eq!(query("/?k=abc&x=1", "x").as_deref(), Some("1"));
        assert_eq!(percent_decode("%E1%BA%A2nh"), "Ảnh");
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn finds_quick_tunnel_address() {
        let l = "2026-11-02T12:00:00Z INF |  https://brave-cat-river.trycloudflare.com                                     |";
        assert_eq!(find_quick_url(l).as_deref(), Some("https://brave-cat-river.trycloudflare.com"));
        assert_eq!(find_quick_url("INF Requesting new quick Tunnel on trycloudflare.com..."), None);
    }

    #[test]
    fn passwords_are_readable() {
        let p = random_password();
        assert_eq!(p.len(), 11);
        assert!(p.chars().all(|c| c == '-' || c.is_ascii_lowercase() || c.is_ascii_digit()));
        assert_ne!(random_password(), p);
    }
}
