// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rawpick_core::analyze::{Analyzer, Scores};
use rawpick_core::cull::{self, Decision, Settings, Summary, Verdict};
use rawpick_core::export::{self, ExportOptions, ExportReport};
use rawpick_core::ingest::{self, CopyReport};
use rawpick_core::learn::{self, Store};
use rawpick_core::options::{self, FeatureInfo, Suggestion};
use rawpick_core::pipeline::{self, Item, Progress, Timing};
use rawpick_core::{metrics, preview};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

const FACE_MODEL: &str = "face_detection_yunet_2023mar.onnx";

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

struct Session {
    source: PathBuf,
    items: Vec<Item>,
    verdicts: Vec<Verdict>,
    overrides: HashMap<String, Decision>,
    suggestions: Vec<Suggestion>,
    timing: Timing,
    applied: Vec<String>,
}

struct App {
    data_dir: PathBuf,
    analyzer: Mutex<Analyzer>,
    face_warning: Option<String>,
    session: Mutex<Option<Session>>,
    prefs: Mutex<Prefs>,
    cancel: AtomicBool,
    busy: AtomicBool,
}

impl App {
    fn store(&self) -> Store {
        Store::new(learn::default_dir(&self.data_dir))
    }

    fn save_prefs(&self, p: &Prefs) {
        let _ = std::fs::create_dir_all(&self.data_dir);
        let _ = std::fs::write(self.data_dir.join("settings.json"), serde_json::to_vec_pretty(p).unwrap_or_default());
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

    /// Re-decide with the current settings and keep Sơn's manual choices.
    fn recull(&self, s: &mut Session) {
        let prefs = self.prefs.lock().unwrap().clone();
        let prof = self.store().load(&prefs.profile);
        s.verdicts = pipeline::decide(&s.items, &prefs.settings, Some(&prof));
        pipeline::apply_overrides(&mut s.verdicts, &s.items, &s.overrides);
    }
}

/// Guard so two long jobs never run at once.
struct Busy<'a>(&'a AtomicBool);
impl<'a> Busy<'a> {
    fn take(b: &'a AtomicBool) -> Result<Self, String> {
        if b.swap(true, Ordering::SeqCst) {
            return Err("Đang có việc khác chạy, đợi xong hoặc bấm Dừng".into());
        }
        Ok(Self(b))
    }
}
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[derive(Serialize, Clone)]
struct ProgressEvent {
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

#[derive(Serialize)]
struct SessionView {
    source: String,
    out_dir: String,
    photos: Vec<PhotoView>,
    verdicts: Vec<Verdict>,
    summary: Summary,
    suggestions: Vec<Suggestion>,
    applied: Vec<String>,
    timing: Timing,
    profile: ProfileView,
    prefs: Prefs,
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
    let prefs = app.prefs.lock().unwrap().clone();
    SessionView {
        source: s.source.display().to_string(),
        out_dir: export::default_out_dir(&s.source).display().to_string(),
        photos: s.items.iter().enumerate().map(|(i, it)| photo_view(i, it)).collect(),
        verdicts: s.verdicts.clone(),
        summary: cull::summarize(&s.verdicts),
        suggestions: s.suggestions.clone(),
        applied: s.applied.clone(),
        timing: s.timing.clone(),
        profile: app.profile_view(&prefs.profile),
        prefs,
    }
}

fn emitter(app: &AppHandle) -> impl Fn(usize, usize, &str) + Sync + '_ {
    move |done, total, stage| {
        let _ = app.emit("progress", ProgressEvent { done, total, stage: stage.to_string() });
    }
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
    prefs: Prefs,
    profiles: Vec<String>,
    profile: ProfileView,
}

#[tauri::command]
fn app_info(app: State<'_, Arc<App>>) -> AppInfo {
    let prefs = app.prefs.lock().unwrap().clone();
    let mut profiles = app.store().list();
    if !profiles.contains(&prefs.profile) {
        profiles.insert(0, prefs.profile.clone());
    }
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        threads: pipeline::threads(),
        data_dir: app.data_dir.display().to_string(),
        face_ok: app.face_warning.is_none(),
        face_warning: app.face_warning.clone(),
        catalog: options::catalog(),
        criteria: rawpick_core::analyze::CRITERIA.to_vec(),
        profile: app.profile_view(&prefs.profile),
        prefs,
        profiles,
    }
}

/// Save settings; if a folder is open, re-decide instantly (no re-analysis).
#[tauri::command]
fn save_prefs(app: State<'_, Arc<App>>, prefs: Prefs) -> Option<VerdictsView> {
    app.save_prefs(&prefs);
    *app.prefs.lock().unwrap() = prefs;
    let mut g = app.session.lock().unwrap();
    let s = g.as_mut()?;
    app.recull(s);
    Some(VerdictsView { verdicts: s.verdicts.clone(), summary: cull::summarize(&s.verdicts) })
}

#[tauri::command]
fn profile_info(app: State<'_, Arc<App>>, name: String) -> ProfileView {
    app.profile_view(&name)
}

#[tauri::command]
async fn analyze(handle: AppHandle, source: String) -> Result<SessionView, String> {
    let app = handle.state::<Arc<App>>().inner().clone();
    let h2 = handle.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _busy = Busy::take(&app.busy)?;
        app.cancel.store(false, Ordering::SeqCst);
        let source = PathBuf::from(source);
        if !source.is_dir() {
            return Err(format!("Không tìm thấy thư mục {}", source.display()));
        }
        let mut prefs = app.prefs.lock().unwrap().clone();
        let work = pipeline::work_dir(&app.data_dir, &source);
        let thumbs = work.join("thumbs");
        std::fs::create_dir_all(&thumbs).map_err(|e| e.to_string())?;
        let on_step = emitter(&h2);
        let p = Progress { on_step: &on_step, cancel: &app.cancel };
        let (items, timing) = {
            let mut an = app.analyzer.lock().unwrap();
            an.thumbs_dir = Some(thumbs);
            pipeline::analyze_all(&source, prefs.recursive, &an, Some(&work), &p).map_err(|e| format!("{e:#}"))?
        };
        if items.is_empty() {
            return Err("Thư mục không có ảnh RAW hoặc JPEG nào".into());
        }
        let suggestions = pipeline::suggestions(&items, &source);
        let applied = options::auto_apply(&mut prefs.settings.options, &suggestions);
        if !applied.is_empty() {
            app.save_prefs(&prefs);
            *app.prefs.lock().unwrap() = prefs;
        }
        let mut s = Session { source, items, verdicts: vec![], overrides: HashMap::new(), suggestions, timing, applied };
        app.recull(&mut s);
        let view = session_view(&app, &s);
        *app.session.lock().unwrap() = Some(s);
        Ok(view)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Sơn's manual choice for one photo; `None` returns it to the AI's decision.
#[tauri::command]
fn set_decision(app: State<'_, Arc<App>>, idx: usize, decision: Option<Decision>) -> Result<VerdictsView, String> {
    let mut g = app.session.lock().unwrap();
    let s = g.as_mut().ok_or("Chưa mở thư mục nào")?;
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
    Ok(VerdictsView { verdicts: s.verdicts.clone(), summary: cull::summarize(&s.verdicts) })
}

/// Teach the current event type's profile from this folder's final choices.
fn learn_from_session(app: &App, s: &Session) -> ProfileView {
    let name = app.prefs.lock().unwrap().profile.clone();
    let store = app.store();
    let mut p = store.load(&name);
    p.add(pipeline::training_samples(&s.items, &s.verdicts));
    p.train();
    let _ = store.save(&p);
    app.profile_view(&name)
}

#[tauri::command]
fn train(app: State<'_, Arc<App>>) -> Result<ProfileView, String> {
    let g = app.session.lock().unwrap();
    let s = g.as_ref().ok_or("Chưa mở thư mục nào")?;
    Ok(learn_from_session(&app, s))
}

#[derive(Serialize)]
struct ExportResult {
    report: ExportReport,
    profile: ProfileView,
}

#[tauri::command]
async fn export_folder(handle: AppHandle, out_dir: Option<String>) -> Result<ExportResult, String> {
    let app = handle.state::<Arc<App>>().inner().clone();
    let h2 = handle.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _busy = Busy::take(&app.busy)?;
        app.cancel.store(false, Ordering::SeqCst);
        let g = app.session.lock().unwrap();
        let s = g.as_ref().ok_or("Chưa mở thư mục nào")?;
        let mut opts = app.prefs.lock().unwrap().export.clone();
        opts.report = opts.report && app.prefs.lock().unwrap().settings.options.report;
        let out = out_dir.map(PathBuf::from).unwrap_or_else(|| export::default_out_dir(&s.source));
        let on_step = emitter(&h2);
        let p = Progress { on_step: &on_step, cancel: &app.cancel };
        let report = pipeline::export_all(&s.source, &s.items, &s.verdicts, &out, &opts, &p).map_err(|e| format!("{e:#}"))?;
        // Every export teaches the AI what Sơn finally kept.
        let profile = learn_from_session(&app, s);
        Ok(ExportResult { report, profile })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn copy_card(handle: AppHandle, card: String, dest: String, backup: Option<String>) -> Result<CopyReport, String> {
    let app = handle.state::<Arc<App>>().inner().clone();
    let h2 = handle.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _busy = Busy::take(&app.busy)?;
        app.cancel.store(false, Ordering::SeqCst);
        let on_step = emitter(&h2);
        ingest::safe_copy(Path::new(&card), Path::new(&dest), backup.as_deref().map(Path::new), &|n, t| on_step(n, t, "Đang chép thẻ"), &app.cancel)
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn cancel(app: State<'_, Arc<App>>) {
    app.cancel.store(true, Ordering::SeqCst);
}

/// A larger preview for the detail view, cached next to the thumbnails.
#[tauri::command]
async fn big_preview(handle: AppHandle, idx: usize) -> Result<String, String> {
    let app = handle.state::<Arc<App>>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (path, work) = {
            let g = app.session.lock().unwrap();
            let s = g.as_ref().ok_or("Chưa mở thư mục nào")?;
            (s.items.get(idx).ok_or("Sai ảnh")?.path.clone(), pipeline::work_dir(&app.data_dir, &s.source))
        };
        let out = work.join("big").join(format!("{idx:06}.jpg"));
        if !out.exists() {
            let pv = preview::load(&path, 1800).map_err(|e| format!("{e:#}"))?;
            let img = metrics::resize_long(&pv.rgb, 1800);
            std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
            img.save_with_format(&out, image::ImageFormat::Jpeg).map_err(|e| e.to_string())?;
        }
        Ok(out.display().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn default_out_dir(source: String) -> String {
    export::default_out_dir(Path::new(&source)).display().to_string()
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
            let prefs: Prefs = std::fs::read(data_dir.join("settings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            let model = find_model(app.handle(), &data_dir);
            let (analyzer, face_warning) = Analyzer::new(model.as_deref(), None);
            app.manage(Arc::new(App {
                data_dir,
                analyzer: Mutex::new(analyzer),
                face_warning,
                session: Mutex::new(None),
                prefs: Mutex::new(prefs),
                cancel: AtomicBool::new(false),
                busy: AtomicBool::new(false),
            }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            save_prefs,
            profile_info,
            analyze,
            set_decision,
            train,
            export_folder,
            copy_card,
            cancel,
            big_preview,
            default_out_dir
        ])
        .run(tauri::generate_context!())
        .expect("không khởi động được SonVu Raw Pick");
}
