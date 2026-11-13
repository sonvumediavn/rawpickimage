//! Command-line version of the culler, handy for testing speed on a real folder:
//!
//!   rawpick cull "D:\Anh\Dam-cuoi" --model models/face_detection_yunet_2023mar.onnx
//!   rawpick copy E:\ D:\Anh\Dam-cuoi --backup F:\Sao-luu

use clap::{Parser, Subcommand, ValueEnum};
use rawpick_core::analyze::Analyzer;
use rawpick_core::cull::{self, Preset, Settings};
use rawpick_core::export::{self, ExportOptions};
use rawpick_core::{ingest, learn, pipeline};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

#[derive(Parser)]
#[command(name = "rawpick", version, about = "Lọc ảnh sự kiện tự động, chạy bằng CPU")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, ValueEnum)]
enum PresetArg {
    Balanced,
    Event,
    Landscape,
    Portrait,
}

#[derive(Subcommand)]
enum Cmd {
    /// Analyse a folder, decide and (unless --dry-run) export the result folders.
    Cull {
        dir: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
        /// YuNet face model (.onnx); without it the "people" criterion is skipped.
        #[arg(long)]
        model: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "event")]
        preset: PresetArg,
        #[arg(long, default_value_t = 7.5)]
        threshold: f32,
        #[arg(long)]
        max: Option<usize>,
        #[arg(short, long)]
        recursive: bool,
        #[arg(long)]
        moments: bool,
        #[arg(long)]
        fix: bool,
        #[arg(long)]
        no_dedupe: bool,
        /// Learned profile to use (event type), stored in --data.
        #[arg(long)]
        profile: Option<String>,
        /// Folder for cache, thumbnails and AI profiles.
        #[arg(long)]
        data: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
        /// Print every decision as JSON lines.
        #[arg(long)]
        json: bool,
    },
    /// Copy a memory card with checksum verification (and an optional backup).
    Copy {
        card: PathBuf,
        dest: PathBuf,
        #[arg(long)]
        backup: Option<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    let cancel = AtomicBool::new(false);
    match Cli::parse().cmd {
        Cmd::Copy { card, dest, backup } => {
            let r = ingest::safe_copy(&card, &dest, backup.as_deref(), &|n, t| eprint!("\rĐang chép {n}/{t}"), &cancel)?;
            eprintln!();
            println!("Đã chép {} file ({:.2} GB), bỏ qua {} file đã có, {} lỗi", r.copied, r.bytes as f64 / 1e9, r.skipped_same, r.errors.len());
            for e in r.errors {
                println!("  LỖI: {e}");
            }
        }
        Cmd::Cull { dir, out, model, preset, threshold, max, recursive, moments, fix, no_dedupe, profile, data, dry_run, json } => {
            let work = data.as_ref().map(|d| pipeline::work_dir(d, &dir));
            let thumbs = work.as_ref().map(|w| w.join("thumbs"));
            if let Some(t) = &thumbs {
                std::fs::create_dir_all(t)?;
            }
            let (analyzer, warn) = Analyzer::new(model.as_deref(), thumbs);
            if let Some(w) = warn {
                eprintln!("Lưu ý: {w}");
            }
            let prog = |n: usize, t: usize, what: &str| eprint!("\r{what} {n}/{t}   ");
            let p = pipeline::Progress { on_step: &prog, cancel: &cancel };
            let (items, timing) = pipeline::analyze_all(&dir, recursive, &analyzer, work.as_deref(), &p)?;
            eprintln!();
            eprintln!(
                "Phân tích {} ảnh trong {:.1} giây ({:.0} ms/ảnh, {} luồng, {} lấy từ bộ nhớ đệm)",
                timing.photos,
                timing.seconds,
                timing.per_photo_ms,
                pipeline::threads(),
                timing.from_cache
            );
            let mut s = Settings {
                weights: match preset {
                    PresetArg::Balanced => Preset::Balanced,
                    PresetArg::Event => Preset::Event,
                    PresetArg::Landscape => Preset::Landscape,
                    PresetArg::Portrait => Preset::Portrait,
                }
                .weights(),
                threshold,
                max_selected: max,
                ..Default::default()
            };
            s.options.moments = moments;
            s.options.fix_instead_of_reject = fix;
            s.options.dedupe = !no_dedupe;
            let prof = match (&profile, &data) {
                (Some(name), Some(d)) => Some(learn::Store::new(learn::default_dir(d)).load(name)),
                _ => None,
            };
            let verdicts = pipeline::decide(&items, &s, prof.as_ref());
            for sug in pipeline::suggestions(&items, &dir) {
                eprintln!("Gợi ý: [{}] {}", sug.key, sug.reason);
            }
            if json {
                for (it, v) in items.iter().zip(&verdicts) {
                    println!(
                        "{}",
                        serde_json::json!({"file": it.path, "decision": v.decision, "score": v.score, "scores": it.analysis.as_ref().map(|a| a.scores), "notes": v.notes, "fixes": v.fixes, "error": it.error})
                    );
                }
            }
            let sum = cull::summarize(&verdicts);
            eprintln!("Đã chọn {} · Cần xem lại {} · Bị loại {} / {} ảnh", sum.selected, sum.review, sum.rejected, sum.total);
            for (r, n) in &sum.by_reason {
                eprintln!("  - {r}: {n}");
            }
            if !dry_run {
                let out = out.unwrap_or_else(|| export::default_out_dir(&dir));
                let rep = pipeline::export_all(&dir, &items, &verdicts, &out, &ExportOptions::default(), &p)?;
                eprintln!();
                eprintln!("Đã xuất vào {} ({} file, {} lỗi)", rep.out_dir.display(), rep.copied, rep.errors.len());
            }
        }
    }
    Ok(())
}
