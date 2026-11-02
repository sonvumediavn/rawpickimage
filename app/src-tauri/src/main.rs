// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rawpick_core::analyze::{Analyzer, Scores};
use rawpick_core::cull::{self, Decision, Settings, Summary, Verdict};
use rawpick_core::export::{ExportOptions, ExportReport};
use rawpick_core::ingest::{self, CopyReport};
use rawpick_core::learn::{self, Store};
use rawpick_core::options::{self, FeatureInfo, Suggestion};
use rawpick_core::pipeline::{self, Item, Progress, Timing};
use rawpick_core::{metrics, preview};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

mod web;

/// Long side of the preview shown at fit size.
const BIG_LONG: u32 = 2560;
const FACE_MODEL: &str = "face_detection_yunet_2023mar.onnx";

/// Settings of one session. New sessions start from the last settings used.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Prefs {
    settings: Settings,
    export: ExportOptions,
    /// Event type whose learned profile is used and trained.
    profile: String,
    recursive: bool,
    preset: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Self { settings: Settings::default(), export: ExportOptions::default(), profile: "Sự kiện chung".into(), recursive: true, preset: "event".into() }
    }
}

/// App-wide settings file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Global {
    /// Defaults for a new session.
    prefs: Prefs,
    /// Folder last chosen for exports.
    export_parent: Option<PathBuf>,
    /// The web version served by this app.
    web: web::WebPrefs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Phase {
    Empty,
    Waiting,
    Analyzing,
    Ready,
    Exporting,
    Failed,
}

struct Session {
    id: u32,
    name: String,
    source: Option<PathBuf>,
    prefs: Prefs,
    phase: Phase,
    error: Option<String>,
    items: Vec<Item>,
    verdicts: Vec<Verdict>,
    overrides: HashMap<String, Decision>,
    suggestions: Vec<Suggestion>,
    timing: Option<Timing>,
    applied: Vec<String>,
    last_export: Option<PathBuf>,
    cancel: Arc<AtomicBool>,
}

/// What survives a restart: the folder, its settings and Sơn's manual choices.
#[derive(Serialize, Deserialize)]
struct SavedSession {
    id: u32,
    name: String,
    source: Option<PathBuf>,
    prefs: Prefs,
    overrides: HashMap<String, Decision>,
    last_export: Option<PathBuf>,
}

impl Session {
    fn new(id: u32, prefs: Prefs) -> Self {
        Self {
            id,
            name: format!("Phiên {id}"),
            source: None,
            prefs,
            phase: Phase::Empty,
            error: None,
            items: vec![],
            verdicts: vec![],
            overrides: HashMap::new(),
            suggestions: vec![],
            timing: None,
            applied: vec![],
            last_export: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

struct App {
    data_dir: PathBuf,
    /// One analysis at a time (old CPUs): waiting on this lock is the queue.
    analyzer: Mutex<Analyzer>,
    face_warning: Option<String>,
    sessions: Mutex<BTreeMap<u32, Session>>,
    next_id: AtomicU32,
    global: Mutex<Global>,
    card_cancel: AtomicBool,
    card_busy: AtomicBool,
    /// The desktop window; events also go to every open web page.
    handle: std::sync::OnceLock<AppHandle>,
    web: web::Web,
}

impl App {
    fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) {
        if let Some(h) = self.handle.get() {
            let _ = h.emit(event, payload.clone());
        }
        self.web.broadcast(event, &payload);
    }

    fn store(&self) -> Store {
        Store::new(learn::default_dir(&self.data_dir))
    }

    fn save_global(&self) {
        let g = self.global.lock().unwrap().clone();
        let _ = std::fs::create_dir_all(&self.data_dir);
        let _ = std::fs::write(self.data_dir.join("settings.json"), serde_json::to_vec_pretty(&g).unwrap_or_default());
    }

    fn save_sessions(&self, map: &BTreeMap<u32, Session>) {
        let saved: Vec<SavedSession> = map
            .values()
            .map(|s| SavedSession {
                id: s.id,
                name: s.name.clone(),
                source: s.source.clone(),
                prefs: s.prefs.clone(),
                overrides: s.overrides.clone(),
                last_export: s.last_export.clone(),
            })
            .collect();
        let _ = std::fs::write(self.data_dir.join("sessions.json"), serde_json::to_vec_pretty(&saved).unwrap_or_default());
    }

    fn profile_view(&self, name: &str) -> ProfileView {
        let p = self.store().load(name);
        ProfileView {
            name: p.name.clone(),
            samples: p.samples.len(),
            ready: p.ready(),
            accuracy: p.ready().then_some(p.accuracy),
            agreement: p.agreement(),
            explain: p.explain(),
            min_samples: learn::MIN_SAMPLES,
        }
    }

    /// Re-decide with the session's settings and keep Sơn's manual choices.
    fn recull(&self, s: &mut Session) {
        let prof = self.store().load(&s.prefs.profile);
        s.verdicts = pipeline::decide(&s.items, &s.prefs.settings, Some(&prof));
        pipeline::apply_overrides(&mut s.verdicts, &s.items, &s.overrides);
    }
}

#[derive(Serialize, Clone)]
struct ProgressEvent {
    session: Option<u32>,
    done: usize,
    total: usize,
    stage: String,
}

#[derive(Serialize)]
struct ProfileView {
    name: String,
    samples: usize,
    ready: bool,
    accuracy: Option<f32>,
    agreement: Option<f32>,
    explain: Vec<String>,
    min_samples: usize,
}

#[derive(Serialize)]
struct FaceBox {
    bbox: [f32; 4],
    closed: bool,
    main: bool,
    issues: Vec<String>,
}

#[derive(Serialize)]
struct PhotoView {
    idx: usize,
    name: String,
    path: String,
    files: Vec<String>,
    thumb: Option<String>,
    camera: String,
    time: Option<f64>,
    width: u32,
    height: u32,
    scores: Option<Scores>,
    /// Per-criterion explanations, in `CRITERIA` order.
    notes: Vec<(String, Vec<String>)>,
    faces: Vec<FaceBox>,
    error: Option<String>,
}

#[derive(Serialize)]
struct VerdictsView {
    verdicts: Vec<Verdict>,
    summary: Summary,
}

/// Small description of a session for the tab bar.
#[derive(Serialize)]
struct SessionMeta {
    id: u32,
    name: String,
    source: Option<String>,
    phase: Phase,
    error: Option<String>,
    summary: Option<Summary>,
    prefs: Prefs,
    last_export: Option<String>,
}

#[derive(Serialize)]
struct SessionView {
    meta: SessionMeta,
    photos: Vec<PhotoView>,
    verdicts: Vec<Verdict>,
    summary: Summary,
    suggestions: Vec<Suggestion>,
    applied: Vec<String>,
    timing: Option<Timing>,
    profile: ProfileView,
}

fn meta(s: &Session) -> SessionMeta {
    SessionMeta {
        id: s.id,
        name: s.name.clone(),
        source: s.source.as_ref().map(|p| p.display().to_string()),
        phase: s.phase,
        error: s.error.clone(),
        summary: (s.phase == Phase::Ready || s.phase == Phase::Exporting).then(|| cull::summarize(&s.verdicts)),
        prefs: s.prefs.clone(),
        last_export: s.last_export.as_ref().map(|p| p.display().to_string()),
    }
}

fn photo_view(idx: usize, it: &Item) -> PhotoView {
    let a = it.analysis.as_ref();
    PhotoView {
        idx,
        name: it.path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        path: it.path.display().to_string(),
        files: it.files.iter().map(|p| p.display().to_string()).collect(),
        thumb: a.and_then(|a| a.thumb.as_ref()).map(|p| p.display().to_string()),
        camera: a.map(|a| a.camera.clone()).unwrap_or_default(),
        time: a.and_then(|a| a.time),
        width: a.map(|a| a.width).unwrap_or(0),
        height: a.map(|a| a.height).unwrap_or(0),
        scores: a.map(|a| a.scores),
        notes: a
            .map(|a| {
                vec![
                    ("exposure".into(), a.exposure.notes.clone()),
                    ("sharp".into(), a.sharp.notes.clone()),
                    ("comp".into(), a.comp.notes.clone()),
                    ("color".into(), a.color.notes.clone()),
                    ("noise".into(), a.noise.notes.clone()),
                    ("people".into(), a.people.notes.clone()),
                ]
            })
            .unwrap_or_default(),
        faces: a
            .map(|a| a.people.faces.iter().map(|f| FaceBox { bbox: f.bbox, closed: f.closed, main: f.main, issues: f.issues.clone() }).collect())
            .unwrap_or_default(),
        error: it.error.clone(),
    }
}

fn session_view(app: &App, s: &Session) -> SessionView {
    SessionView {
        meta: meta(s),
        photos: s.items.iter().enumerate().map(|(i, it)| photo_view(i, it)).collect(),
        verdicts: s.verdicts.clone(),
        summary: cull::summarize(&s.verdicts),
        suggestions: s.suggestions.clone(),
        applied: s.applied.clone(),
        timing: s.timing.clone(),
        profile: app.profile_view(&s.prefs.profile),
    }
}

fn emitter(app: &App, session: Option<u32>) -> impl Fn(usize, usize, &str) + Sync + '_ {
    move |done, total, stage| {
        app.emit("progress", ProgressEvent { session, done, total, stage: stage.to_string() });
    }
}

fn sessions_changed(app: &App) {
    app.emit("sessions-changed", ());
}

fn folder_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

#[derive(Serialize)]
struct AppInfo {
    version: String,
    threads: usize,
    data_dir: String,
    face_ok: bool,
    face_warning: Option<String>,
    catalog: Vec<FeatureInfo>,
    criteria: Vec<&'static str>,
    defaults: Prefs,
    profiles: Vec<String>,
    export_parent: Option<String>,
    sessions: Vec<SessionMeta>,
}

fn op_app_info(app: &Arc<App>) -> AppInfo {
    let g = app.global.lock().unwrap().clone();
    let sessions: Vec<SessionMeta> = app.sessions.lock().unwrap().values().map(meta).collect();
    let mut profiles = app.store().list();
    for s in &sessions {
        if !profiles.contains(&s.prefs.profile) {
            profiles.push(s.prefs.profile.clone());
        }
    }
    if !profiles.contains(&g.prefs.profile) {
        profiles.insert(0, g.prefs.profile.clone());
    }
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        threads: pipeline::threads(),
        data_dir: app.data_dir.display().to_string(),
        face_ok: app.face_warning.is_none(),
        face_warning: app.face_warning.clone(),
        catalog: options::catalog(),
        criteria: rawpick_core::analyze::CRITERIA.to_vec(),
        defaults: g.prefs,
        profiles,
        export_parent: g.export_parent.map(|p| p.display().to_string()),
        sessions,
    }
}

fn op_new_session(app: &Arc<App>) -> SessionMeta {
    let id = app.next_id.fetch_add(1, Ordering::SeqCst);
    let prefs = app.global.lock().unwrap().prefs.clone();
    let mut map = app.sessions.lock().unwrap();
    let s = Session::new(id, prefs);
    let m = meta(&s);
    map.insert(id, s);
    app.save_sessions(&map);
    drop(map);
    sessions_changed(app);
    m
}

fn op_close_session(app: &Arc<App>, id: u32) {
    let mut map = app.sessions.lock().unwrap();
    if let Some(s) = map.remove(&id) {
        s.cancel.store(true, Ordering::SeqCst);
    }
    app.save_sessions(&map);
    drop(map);
    sessions_changed(app);
}

fn op_list_sessions(app: &Arc<App>) -> Vec<SessionMeta> {
    app.sessions.lock().unwrap().values().map(meta).collect()
}

fn op_get_session(app: &Arc<App>, id: u32) -> Result<SessionView, String> {
    let map = app.sessions.lock().unwrap();
    let s = map.get(&id).ok_or("Phiên đã đóng")?;
    Ok(session_view(&app, s))
}

/// Save a session's settings (they also become the defaults for new sessions)
/// and re-decide instantly; no re-analysis needed.
fn op_save_prefs(app: &Arc<App>, id: Option<u32>, prefs: Prefs) -> Option<VerdictsView> {
    app.global.lock().unwrap().prefs = prefs.clone();
    app.save_global();
    let mut map = app.sessions.lock().unwrap();
    let s = map.get_mut(&id?)?;
    s.prefs = prefs;
    let out = if s.phase == Phase::Ready {
        app.recull(s);
        Some(VerdictsView { verdicts: s.verdicts.clone(), summary: cull::summarize(&s.verdicts) })
    } else {
        None
    };
    app.save_sessions(&map);
    out
}

fn op_profile_info(app: &Arc<App>, name: String) -> ProfileView {
    app.profile_view(&name)
}

/// Analyse a folder in a session. With `source` = None the session's saved folder is reopened
/// (instant thanks to the cache). Several calls queue up and run one after another.
fn op_analyze(app: &Arc<App>, id: u32, source: Option<String>) -> Result<SessionView, String> {
    let app = app.clone();
    (move || {
        let (source, prefs, cancel) = {
            let mut map = app.sessions.lock().unwrap();
            let s = map.get_mut(&id).ok_or("Phiên đã đóng")?;
            if matches!(s.phase, Phase::Waiting | Phase::Analyzing | Phase::Exporting) {
                return Err("Phiên này đang chạy, đợi xong hoặc bấm Dừng".to_string());
            }
            let src = match source {
                Some(p) => PathBuf::from(p),
                None => s.source.clone().ok_or("Phiên chưa có thư mục")?,
            };
            if !src.is_dir() {
                s.phase = if s.items.is_empty() { Phase::Failed } else { Phase::Ready };
                s.error = Some(format!("Không tìm thấy thư mục {}", src.display()));
                return Err(s.error.clone().unwrap());
            }
            if s.source.as_ref() != Some(&src) {
                s.overrides.clear();
            }
            s.name = folder_name(&src);
            s.source = Some(src.clone());
            s.phase = Phase::Waiting;
            s.error = None;
            s.cancel.store(false, Ordering::SeqCst);
            let out = (src, s.prefs.clone(), s.cancel.clone());
            app.save_sessions(&map);
            out
        };
        sessions_changed(&app);
        let on_step = emitter(&app, Some(id));
        on_step(0, 0, "Đang chờ phiên khác phân tích xong");

        let work = pipeline::work_dir(&app.data_dir, &source);
        let result = (|| {
            let thumbs = work.join("thumbs");
            std::fs::create_dir_all(&thumbs).map_err(|e| e.to_string())?;
            let mut an = app.analyzer.lock().unwrap();
            if cancel.load(Ordering::SeqCst) {
                return Err("Đã dừng".to_string());
            }
            if let Some(s) = app.sessions.lock().unwrap().get_mut(&id) {
                s.phase = Phase::Analyzing;
            }
            sessions_changed(&app);
            an.thumbs_dir = Some(thumbs);
            let p = Progress { on_step: &on_step, cancel: &cancel };
            pipeline::analyze_all(&source, prefs.recursive, &an, Some(&work), &p).map_err(|e| format!("{e:#}"))
        })();

        let mut map = app.sessions.lock().unwrap();
        let s = map.get_mut(&id).ok_or("Phiên đã đóng")?;
        let (items, timing) = match result {
            Ok(r) if cancel.load(Ordering::SeqCst) => {
                drop(r);
                s.phase = if s.items.is_empty() { Phase::Empty } else { Phase::Ready };
                drop(map);
                sessions_changed(&app);
                return Err("Đã dừng".into());
            }
            Ok(r) => r,
            Err(e) => {
                s.phase = Phase::Failed;
                s.error = Some(e.clone());
                drop(map);
                sessions_changed(&app);
                return Err(e);
            }
        };
        if items.is_empty() {
            s.phase = Phase::Failed;
            s.error = Some("Thư mục không có ảnh RAW hoặc JPEG nào".into());
            let e = s.error.clone().unwrap();
            drop(map);
            sessions_changed(&app);
            return Err(e);
        }
        s.suggestions = pipeline::suggestions(&items, &source);
        s.applied = options::auto_apply(&mut s.prefs.settings.options, &s.suggestions);
        s.items = items;
        s.timing = Some(timing);
        s.phase = Phase::Ready;
        app.recull(s);
        let view = session_view(&app, s);
        app.save_sessions(&map);
        drop(map);
        sessions_changed(&app);
        Ok(view)
    })()
}

/// Sơn's manual choice for one photo; `None` returns it to the AI's decision.
fn op_set_decision(app: &Arc<App>, id: u32, idx: usize, decision: Option<Decision>) -> Result<VerdictsView, String> {
    let mut map = app.sessions.lock().unwrap();
    let s = map.get_mut(&id).ok_or("Phiên đã đóng")?;
    let key = s.items.get(idx).ok_or("Sai ảnh")?.path.display().to_string();
    match decision {
        Some(d) => {
            s.overrides.insert(key, d);
        }
        None => {
            s.overrides.remove(&key);
        }
    }
    app.recull(s);
    let v = VerdictsView { verdicts: s.verdicts.clone(), summary: cull::summarize(&s.verdicts) };
    app.save_sessions(&map);
    Ok(v)
}

/// Teach the session's event-type profile from its final choices.
fn learn_from(app: &App, profile: &str, items: &[Item], verdicts: &[Verdict]) -> ProfileView {
    let store = app.store();
    let mut p = store.load(profile);
    p.add(pipeline::training_samples(items, verdicts));
    p.train();
    let _ = store.save(&p);
    app.profile_view(profile)
}

fn op_train(app: &Arc<App>, id: u32) -> Result<ProfileView, String> {
    let map = app.sessions.lock().unwrap();
    let s = map.get(&id).ok_or("Phiên đã đóng")?;
    Ok(learn_from(&app, &s.prefs.profile, &s.items, &s.verdicts))
}

#[derive(Serialize)]
struct ExportResult {
    report: ExportReport,
    profile: ProfileView,
}

/// The folder an export into `parent` will create.
fn op_export_target(app: &Arc<App>, id: u32, parent: String) -> Result<String, String> {
    let map = app.sessions.lock().unwrap();
    let s = map.get(&id).ok_or("Phiên đã đóng")?;
    let src = s.source.as_ref().ok_or("Phiên chưa có thư mục")?;
    Ok(Path::new(&parent).join(format!("{}_da-loc", folder_name(src))).display().to_string())
}

/// Export into `<parent>/<folder name>_da-loc`, `parent` chosen by Sơn.
fn op_export_folder(app: &Arc<App>, id: u32, parent: String) -> Result<ExportResult, String> {
    let app = app.clone();
    (move || {
        let parent = PathBuf::from(parent);
        if !parent.is_dir() {
            return Err(format!("Không tìm thấy thư mục {}", parent.display()));
        }
        // Snapshot the session so it stays usable (and other sessions run) while copying.
        let (source, items, verdicts, opts, profile, cancel) = {
            let mut map = app.sessions.lock().unwrap();
            let s = map.get_mut(&id).ok_or("Phiên đã đóng")?;
            if s.phase != Phase::Ready {
                return Err("Phiên này chưa phân tích xong".into());
            }
            s.phase = Phase::Exporting;
            s.cancel.store(false, Ordering::SeqCst);
            let mut opts = s.prefs.export.clone();
            opts.report = opts.report && s.prefs.settings.options.report;
            (s.source.clone().unwrap(), s.items.clone(), s.verdicts.clone(), opts, s.prefs.profile.clone(), s.cancel.clone())
        };
        sessions_changed(&app);
        let out = parent.join(format!("{}_da-loc", folder_name(&source)));
        let on_step = emitter(&app, Some(id));
        let p = Progress { on_step: &on_step, cancel: &cancel };
        let res = pipeline::export_all(&source, &items, &verdicts, &out, &opts, &p).map_err(|e| format!("{e:#}"));
        {
            let mut map = app.sessions.lock().unwrap();
            if let Some(s) = map.get_mut(&id) {
                s.phase = Phase::Ready;
                if res.is_ok() {
                    s.last_export = Some(out.clone());
                }
            }
            app.save_sessions(&map);
        }
        {
            let mut g = app.global.lock().unwrap();
            g.export_parent = Some(parent);
        }
        app.save_global();
        sessions_changed(&app);
        let report = res?;
        // Every export teaches the AI what Sơn finally kept.
        let profile = learn_from(&app, &profile, &items, &verdicts);
        Ok(ExportResult { report, profile })
    })()
}

#[derive(Serialize)]
struct GatherResult {
    report: pipeline::GatherReport,
    view: SessionView,
}

/// Move the selected photos into a sub-folder of the source folder (no export).
fn op_gather_selected(app: &Arc<App>, id: u32, sub: String) -> Result<GatherResult, String> {
    let mut map = app.sessions.lock().unwrap();
    let s = map.get_mut(&id).ok_or("Phiên đã đóng")?;
    if s.phase != Phase::Ready {
        return Err("Phiên này chưa phân tích xong".into());
    }
    let source = s.source.clone().ok_or("Phiên chưa có thư mục")?;
    let old: Vec<String> = s.items.iter().map(|i| i.path.display().to_string()).collect();
    let report = pipeline::gather_selected(&source, &mut s.items, &s.verdicts, &sub).map_err(|e| format!("{e:#}"))?;
    // Manual choices follow the files to their new place.
    for (i, o) in old.iter().enumerate() {
        let new = s.items[i].path.display().to_string();
        if *o != new {
            if let Some(d) = s.overrides.remove(o) {
                s.overrides.insert(new, d);
            }
        }
    }
    app.recull(s);
    let view = session_view(&app, s);
    app.save_sessions(&map);
    Ok(GatherResult { report, view })
}

// ---------------------------------------------------------------- chat

/// Run Claude Code (signed in with the user's own Claude account) in print mode.
/// Unix: through a login shell so the user's PATH (Homebrew, ~/.local/bin, nvm…) applies,
/// since apps started from the Dock/Finder don't get it.
fn claude_command(args: &str) -> std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg(format!("claude {args}"));
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        c
    }
    #[cfg(not(windows))]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let mut c = std::process::Command::new(shell);
        let home = std::env::var("HOME").unwrap_or_default();
        c.arg("-lc").arg(format!("PATH=\"$PATH:{home}/.local/bin:{home}/.claude/local:/opt/homebrew/bin:/usr/local/bin\" exec claude {args}"));
        c
    }
}

fn run_with_timeout(mut cmd: std::process::Command, input: Option<String>, secs: u64) -> Result<std::process::Output, String> {
    use std::io::Write;
    use std::process::Stdio;
    cmd.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("Không chạy được Claude Code: {e}"))?;
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        let _ = stdin.write_all(text.as_bytes());
    }
    let id = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(std::time::Duration::from_secs(secs)) {
        Ok(r) => r.map_err(|e| e.to_string()),
        Err(_) => {
            #[cfg(windows)]
            let _ = std::process::Command::new("taskkill").args(["/PID", &id.to_string(), "/T", "/F"]).output();
            #[cfg(not(windows))]
            let _ = std::process::Command::new("kill").arg(id.to_string()).output();
            Err("Claude trả lời quá lâu, thử lại sau".into())
        }
    }
}

#[derive(Serialize)]
struct ClaudeStatus {
    found: bool,
    version: Option<String>,
    hint: String,
}

fn op_claude_status() -> ClaudeStatus {
    match run_with_timeout(claude_command("--version"), None, 20) {
        Ok(o) if o.status.success() => ClaudeStatus {
            found: true,
            version: Some(String::from_utf8_lossy(&o.stdout).trim().to_string()),
            hint: "Dùng Claude Code đã đăng nhập trên máy này (tài khoản Claude của bạn)".into(),
        },
        _ => ClaudeStatus {
            found: false,
            version: None,
            hint: "Chưa thấy Claude Code trên máy này. Cài từ claude.com/claude-code, mở Terminal gõ «claude» và đăng nhập một lần, rồi mở lại ứng dụng.".into(),
        },
    }
}

#[derive(Serialize)]
struct ChatResult {
    reply: String,
    changes: Vec<String>,
    by: &'static str,
    prefs: Option<Prefs>,
    verdicts: Option<VerdictsView>,
    before: Option<usize>,
    after: Option<usize>,
}

fn chat_context(s: &Session) -> Option<rawpick_core::chat::Context> {
    if s.items.is_empty() {
        return None;
    }
    let mut sum = [0f32; 6];
    let mut n = [0usize; 6];
    let mut cams = std::collections::BTreeSet::new();
    let mut times = vec![];
    for a in s.items.iter().filter_map(|i| i.analysis.as_ref()) {
        for (k, c) in rawpick_core::analyze::CRITERIA.iter().enumerate() {
            if let Some(v) = a.scores.get(c) {
                sum[k] += v;
                n[k] += 1;
            }
        }
        cams.insert(a.camera.clone());
        if let Some(t) = a.time {
            times.push(t);
        }
    }
    times.sort_by(f64::total_cmp);
    let mut mean = [0f32; 6];
    for k in 0..6 {
        mean[k] = if n[k] > 0 { (sum[k] / n[k] as f32 * 10.0).round() / 10.0 } else { 0.0 };
    }
    Some(rawpick_core::chat::Context {
        summary: cull::summarize(&s.verdicts),
        mean_scores: mean,
        with_people: n[5],
        cameras: cams.into_iter().collect(),
        hours: times.last().zip(times.first()).map(|(b, a)| ((b - a) / 3600.0) as f32).unwrap_or(0.0),
    })
}

/// One chat message: understand it (offline, or with Claude), change the session's settings,
/// re-decide, and say what changed.
fn op_chat(app: &Arc<App>, id: Option<u32>, message: String, use_claude: bool) -> Result<ChatResult, String> {
    use rawpick_core::chat;
    let app = app.clone();
    (move || {
        let (mut prefs, ctx) = {
            let map = app.sessions.lock().unwrap();
            match id.and_then(|i| map.get(&i)) {
                Some(s) => (s.prefs.clone(), chat_context(s)),
                None => (app.global.lock().unwrap().prefs.clone(), None),
            }
        };
        let (patch, mut notes, by) = if use_claude {
            let ready = app.store().load(&prefs.profile).ready();
            let prompt = chat::claude_prompt(&message, &prefs.settings, &prefs.preset, ctx.as_ref(), ready);
            let dir = app.data_dir.join("claude-chat");
            let _ = std::fs::create_dir_all(&dir);
            // Answer-only: no shell, no file edits, no web. Runs in an empty folder.
            let mut cmd = claude_command("-p --output-format json --disallowedTools Bash Edit Write NotebookEdit WebFetch WebSearch");
            cmd.current_dir(&dir);
            let out = run_with_timeout(cmd, Some(prompt), 180)?;
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            if !out.status.success() && stdout.trim().is_empty() {
                let err = String::from_utf8_lossy(&out.stderr);
                return Err(format!("Claude Code báo lỗi: {}. Nếu chưa đăng nhập, mở Terminal gõ «claude» để đăng nhập.", err.trim().chars().take(300).collect::<String>()));
            }
            let ans = chat::parse_claude_answer(&stdout).map_err(|e| format!("{e}. Nếu chưa đăng nhập, mở Terminal gõ «claude» để đăng nhập."))?;
            (ans.patch, vec![ans.reply], "claude")
        } else {
            let u = chat::understand(&message, &prefs.settings.weights);
            (u.patch, u.notes, "local")
        };
        if patch.is_empty() {
            let reply = if by == "local" { chat::HELP.to_string() } else { notes.join("\n") };
            return Ok(ChatResult { reply, changes: vec![], by, prefs: None, verdicts: None, before: None, after: None });
        }
        let changes = patch.apply(&mut prefs.settings, &mut prefs.preset);
        {
            let mut g = app.global.lock().unwrap();
            g.prefs = prefs.clone();
        }
        app.save_global();
        let mut map = app.sessions.lock().unwrap();
        let (verdicts, before, after) = match id.and_then(|i| map.get_mut(&i)) {
            Some(s) => {
                s.prefs = prefs.clone();
                if s.phase == Phase::Ready {
                    let before = s.verdicts.iter().filter(|v| v.decision == Decision::Selected).count();
                    app.recull(s);
                    let after = s.verdicts.iter().filter(|v| v.decision == Decision::Selected).count();
                    (Some(VerdictsView { verdicts: s.verdicts.clone(), summary: cull::summarize(&s.verdicts) }), Some(before), Some(after))
                } else {
                    (None, None, None)
                }
            }
            None => (None, None, None),
        };
        app.save_sessions(&map);
        drop(map);
        let mut reply = String::new();
        if by == "claude" {
            reply += &notes.remove(0);
        } else {
            reply += "Đã chỉnh theo yêu cầu.";
            for n in notes {
                reply += "\n";
                reply += &n;
            }
        }
        if let (Some(b), Some(a)) = (before, after) {
            reply += &format!("\nẢnh đã chọn: {b} → {a}.");
        }
        Ok(ChatResult { reply, changes, by, prefs: Some(prefs), verdicts, before, after })
    })()
}

fn op_copy_card(app: &Arc<App>, card: String, dest: String, backup: Option<String>) -> Result<CopyReport, String> {
    let app = app.clone();
    (move || {
        if app.card_busy.swap(true, Ordering::SeqCst) {
            return Err("Đang chép một thẻ khác".to_string());
        }
        app.card_cancel.store(false, Ordering::SeqCst);
        let on_step = emitter(&app, None);
        let r = ingest::safe_copy(Path::new(&card), Path::new(&dest), backup.as_deref().map(Path::new), &|n, t| on_step(n, t, "Đang chép thẻ"), &app.card_cancel)
            .map_err(|e| format!("{e:#}"));
        app.card_busy.store(false, Ordering::SeqCst);
        r
    })()
}

/// Stop a session's analysis/export, or the card copy when `id` is None.
fn op_cancel(app: &Arc<App>, id: Option<u32>) {
    match id {
        Some(id) => {
            if let Some(s) = app.sessions.lock().unwrap().get(&id) {
                s.cancel.store(true, Ordering::SeqCst);
            }
        }
        None => app.card_cancel.store(true, Ordering::SeqCst),
    }
}

/// A larger preview for the detail view, cached next to the thumbnails.
/// `full` = the preview at its native size (for checking focus at 100 %).
fn op_big_preview(app: &Arc<App>, id: u32, idx: usize, full: Option<bool>) -> Result<String, String> {
    let app = app.clone();
    let full = full.unwrap_or(false);
    (move || {
        let (path, work) = {
            let map = app.sessions.lock().unwrap();
            let s = map.get(&id).ok_or("Phiên đã đóng")?;
            (s.items.get(idx).ok_or("Sai ảnh")?.path.clone(), pipeline::work_dir(&app.data_dir, s.source.as_ref().ok_or("Phiên chưa có thư mục")?))
        };
        let key = rawpick_core::analyze::file_key(&path);
        // 2560 px fills a Retina screen at fit size; `full` is every pixel the file holds.
        let out = work.join("big").join(if full { format!("{key}-full.jpg") } else { format!("{key}-2560.jpg") });
        if !out.exists() {
            let long = if full { u32::MAX } else { BIG_LONG };
            let pv = preview::load(&path, long).map_err(|e| format!("{e:#}"))?;
            let img = if full || pv.rgb.width().max(pv.rgb.height()) <= BIG_LONG { pv.rgb } else { metrics::resize_long(&pv.rgb, BIG_LONG) };
            std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
            let tmp = out.with_extension("tmp");
            let mut f = std::io::BufWriter::new(std::fs::File::create(&tmp).map_err(|e| e.to_string())?);
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, if full { 97 } else { 92 }).encode_image(&img).map_err(|e| e.to_string())?;
            drop(f);
            std::fs::rename(&tmp, &out).map_err(|e| e.to_string())?;
        }
        Ok(out.display().to_string())
    })()
}

// ---------------------------------------------------------------- one entry point

/// Arguments of every command, as the page sends them (camelCase).
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Args {
    id: Option<u32>,
    idx: Option<usize>,
    source: Option<String>,
    prefs: Option<Prefs>,
    name: Option<String>,
    decision: Option<Decision>,
    parent: Option<String>,
    sub: Option<String>,
    message: Option<String>,
    use_claude: bool,
    card: Option<String>,
    dest: Option<String>,
    backup: Option<String>,
    full: Option<bool>,
    path: Option<String>,
    lan: Option<bool>,
    remote: Option<bool>,
    new_password: bool,
    tunnel_host: Option<String>,
    tunnel_token: Option<String>,
}

fn need<T>(v: Option<T>, what: &str) -> Result<T, String> {
    v.ok_or_else(|| format!("Thiếu {what}"))
}

fn json<T: Serialize>(v: T) -> Result<serde_json::Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

/// Every command of the app, shared by the desktop window and the web version.
fn dispatch(app: &Arc<App>, cmd: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
    let a: Args = if args.is_null() { Args::default() } else { serde_json::from_value(args).map_err(|e| format!("Sai tham số: {e}"))? };
    match cmd {
        "app_info" => json(op_app_info(app)),
        "new_session" => json(op_new_session(app)),
        "close_session" => json(op_close_session(app, need(a.id, "id")?)),
        "list_sessions" => json(op_list_sessions(app)),
        "get_session" => json(op_get_session(app, need(a.id, "id")?)?),
        "save_prefs" => json(op_save_prefs(app, a.id, need(a.prefs, "prefs")?)),
        "profile_info" => json(op_profile_info(app, need(a.name, "name")?)),
        "analyze" => json(op_analyze(app, need(a.id, "id")?, a.source)?),
        "set_decision" => json(op_set_decision(app, need(a.id, "id")?, need(a.idx, "idx")?, a.decision)?),
        "train" => json(op_train(app, need(a.id, "id")?)?),
        "export_target" => json(op_export_target(app, need(a.id, "id")?, need(a.parent, "parent")?)?),
        "export_folder" => json(op_export_folder(app, need(a.id, "id")?, need(a.parent, "parent")?)?),
        "gather_selected" => json(op_gather_selected(app, need(a.id, "id")?, need(a.sub, "sub")?)?),
        "claude_status" => json(op_claude_status()),
        "chat" => json(op_chat(app, a.id, need(a.message, "message")?, a.use_claude)?),
        "copy_card" => json(op_copy_card(app, need(a.card, "card")?, need(a.dest, "dest")?, a.backup)?),
        "cancel" => json(op_cancel(app, a.id)),
        "big_preview" => json(op_big_preview(app, need(a.id, "id")?, need(a.idx, "idx")?, a.full)?),
        "list_dir" => json(web::list_dir(a.path.as_deref())?),
        "open_path" => {
            let p = need(a.path, "path")?;
            tauri_plugin_opener::open_path(&p, None::<&str>).map_err(|e| format!("Không mở được {p}: {e}"))?;
            json(())
        }
        "web_info" => json(web::info(app)),
        "web_set" => json(web::set(app, a.lan, a.remote, a.new_password, a.tunnel_host, a.tunnel_token)?),
        "open_web" => json(web::open(app, &need(a.path, "path")?)?),
        _ => Err(format!("Không có lệnh {cmd}")),
    }
}

/// The desktop window's single command; long work runs off the UI thread.
#[tauri::command]
async fn call(state: State<'_, Arc<App>>, cmd: String, args: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || dispatch(&app, &cmd, args.unwrap_or_default())).await.map_err(|e| e.to_string())?
}

fn find_model(handle: &AppHandle, data_dir: &Path) -> Option<PathBuf> {
    let mut c = vec![];
    if let Ok(r) = handle.path().resource_dir() {
        c.push(r.join("models").join(FACE_MODEL));
    }
    c.push(data_dir.join("models").join(FACE_MODEL));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            c.push(d.join("models").join(FACE_MODEL));
        }
    }
    // Development: repo/models
    c.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models").join(FACE_MODEL));
    c.into_iter().find(|p| p.exists())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let global: Global = std::fs::read(data_dir.join("settings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            let saved: Vec<SavedSession> = std::fs::read(data_dir.join("sessions.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            let mut sessions = BTreeMap::new();
            for s in saved {
                let mut x = Session::new(s.id, s.prefs);
                x.name = s.name;
                x.source = s.source;
                x.overrides = s.overrides;
                x.last_export = s.last_export;
                sessions.insert(x.id, x);
            }
            let next = sessions.keys().max().map(|m| m + 1).unwrap_or(1);
            let model = find_model(app.handle(), &data_dir);
            let (analyzer, face_warning) = Analyzer::new(model.as_deref(), None);
            let app_state = Arc::new(App {
                data_dir,
                analyzer: Mutex::new(analyzer),
                face_warning,
                sessions: Mutex::new(sessions),
                next_id: AtomicU32::new(next),
                global: Mutex::new(global),
                card_cancel: AtomicBool::new(false),
                card_busy: AtomicBool::new(false),
                handle: std::sync::OnceLock::new(),
                web: web::Web::default(),
            });
            let _ = app_state.handle.set(app.handle().clone());
            web::start(&app_state);
            app.manage(app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![call])
        .build(tauri::generate_context!())
        .expect("không khởi động được SonVu Raw Pick")
        .run(|handle, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(app) = handle.try_state::<Arc<App>>() {
                    app.web.stop_tunnel();
                }
            }
        });
}
