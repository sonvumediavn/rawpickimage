//! Writing the result: the chosen shots (RAW + JPEG twin + XMP, byte for byte) go straight into
//! `<event>_anh chon/` inside the event folder; on request also `Can-xem-lai/`, `Bi-loai/<reason>/` and a report.
//! Each copy is verified with CRC32; originals are only removed in move mode, after that check.

use crate::analyze::Analysis;
use crate::cull::{self, Decision, Verdict};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportOptions {
    /// Also copy rejected photos into `Bi-loai/<reason>`.
    pub copy_rejected: bool,
    /// Also copy «Cần xem lại» photos into `Can-xem-lai`.
    pub copy_review: bool,
    /// Write `bao-cao.csv` and the shoot summary `tong-ket.txt`.
    pub report: bool,
    /// Move instead of copy: each original is removed only after its new copy is verified
    /// (or renamed in one step when source and destination are on the same drive).
    pub move_files: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        // Sơn's call (2026-11-03): only the chosen photos, no report files.
        Self { copy_rejected: false, copy_review: false, report: false, move_files: false }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ExportReport {
    pub out_dir: PathBuf,
    pub copied: usize,
    pub skipped_same: usize,
    pub moved: usize,
    pub bytes: u64,
    pub errors: Vec<String>,
}

/// Suffix of the export folder, Sơn's naming (2026-11-04): «261104_van nghe cong doan_anh chon».
pub const OUT_SUFFIX: &str = "_anh chon";

/// Name of the export folder: the source folder's name plus «_anh chon».
pub fn out_name(source: &Path) -> String {
    let name = source.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Anh".into());
    format!("{name}{OUT_SUFFIX}")
}

/// Default output folder: inside the source folder, where the show is archived.
pub fn default_out_dir(source: &Path) -> PathBuf {
    source.join(out_name(source))
}

/// Where a shot goes inside the export folder: chosen ones at the top.
fn sub_folder(d: Decision) -> PathBuf {
    match d {
        Decision::Selected => PathBuf::new(),
        Decision::Review => PathBuf::from("Can-xem-lai"),
        Decision::Rejected(r) => Path::new("Bi-loai").join(r.folder()),
    }
}

pub fn crc_file(p: &Path) -> std::io::Result<(u32, u64)> {
    let mut r = BufReader::with_capacity(1 << 20, File::open(p)?);
    let mut h = crc32fast::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut n = 0u64;
    loop {
        let k = r.read(&mut buf)?;
        if k == 0 {
            break;
        }
        h.update(&buf[..k]);
        n += k as u64;
    }
    Ok((h.finalize(), n))
}

/// Plain-language reason for a file that can't be moved or deleted.
pub fn explain_io(e: &std::io::Error) -> String {
    match (e.kind(), e.raw_os_error()) {
        (_, Some(32)) | (_, Some(33)) => "file đang được chương trình khác mở (Lightroom, Photoshop, Photo Mechanic, trình xem ảnh…), đóng chương trình đó rồi thử lại".into(),
        (std::io::ErrorKind::PermissionDenied, _) => "Windows không cho đổi chỗ file. Thường do file đang mở trong chương trình khác (Lightroom, Photoshop, trình xem ảnh), hoặc phần mềm diệt virus / «Controlled folder access» của Windows Security đang chặn SonVu Raw Pick".into(),
        _ => e.to_string(),
    }
}

fn set_readonly(p: &Path, on: bool) -> std::io::Result<()> {
    let mut perm = std::fs::metadata(p)?.permissions();
    if perm.readonly() != on {
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(on);
        std::fs::set_permissions(p, perm)?;
    }
    Ok(())
}

/// Move one file without changing a byte. A rename when the drive allows it; otherwise a
/// checksum-verified copy, and only then the original is removed. Read-only files (cameras mark
/// "protected" shots that way) keep that mark at the new place. On failure nothing is lost:
/// the original stays where it was.
pub fn move_verified(src: &Path, dst: &Path) -> Result<()> {
    let readonly = std::fs::metadata(src).map(|m| m.permissions().readonly()).unwrap_or(false);
    if readonly {
        let _ = set_readonly(src, false);
    }
    let restore = |p: &Path| {
        if readonly {
            let _ = set_readonly(p, true);
        }
    };
    let mut last = None;
    for attempt in 0..3u64 {
        match std::fs::rename(src, dst) {
            Ok(()) => {
                restore(dst);
                return Ok(());
            }
            Err(e) => {
                // A virus scanner or the thumbnail indexer can hold the file for a moment.
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(150 * (attempt + 1)));
            }
        }
    }
    // Rename refused (other drive, or Windows says no): copy, verify, then remove the original.
    if let Err(e) = copy_verified(src, dst) {
        restore(src);
        let why = last.as_ref().map(explain_io).unwrap_or_default();
        bail!("Không chuyển được {}: {why} ({e:#})", src.display());
    }
    if let Err(e) = std::fs::remove_file(src) {
        // Keep one copy only, at the original place.
        let _ = std::fs::remove_file(dst);
        restore(src);
        bail!("Không chuyển được {}: {}", src.display(), explain_io(&e));
    }
    restore(dst);
    Ok(())
}

/// Copy with CRC check of the written file. Returns bytes copied, or None if an identical file
/// already existed at `dst`.
pub fn copy_verified(src: &Path, dst: &Path) -> Result<Option<u64>> {
    let (crc_src, len) = crc_file(src).with_context(|| format!("đọc {}", src.display()))?;
    if dst.exists() {
        if let Ok((c, l)) = crc_file(dst) {
            if c == crc_src && l == len {
                return Ok(None);
            }
        }
    }
    if let Some(d) = dst.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = dst.with_extension(format!("{}.dang-chep", dst.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default()));
    {
        let mut r = BufReader::with_capacity(1 << 20, File::open(src)?);
        let mut w = BufWriter::with_capacity(1 << 20, File::create(&tmp)?);
        std::io::copy(&mut r, &mut w)?;
        w.flush()?;
        w.get_ref().sync_all()?;
    }
    let (crc_dst, len_dst) = crc_file(&tmp)?;
    if crc_dst != crc_src || len_dst != len {
        let _ = std::fs::remove_file(&tmp);
        bail!("Bản chép của {} bị lỗi (checksum không khớp)", src.display());
    }
    std::fs::rename(&tmp, dst)?;
    if let Ok(m) = std::fs::metadata(src).and_then(|m| m.modified()) {
        let _ = File::options().write(true).open(dst).and_then(|f| f.set_modified(m));
    }
    Ok(Some(len))
}

/// One photo to export.
pub struct Entry<'a> {
    pub files: &'a [PathBuf],
    pub analysis: Option<&'a Analysis>,
    pub verdict: &'a Verdict,
    /// Folder the photo was found in, relative to the source (kept to avoid name clashes).
    pub rel_dir: PathBuf,
}

pub fn export(
    entries: &[Entry],
    out_dir: &Path,
    opts: &ExportOptions,
    progress: &(dyn Fn(usize, usize) + Sync),
    cancel: &AtomicBool,
) -> Result<ExportReport> {
    std::fs::create_dir_all(out_dir).with_context(|| format!("tạo thư mục {}", out_dir.display()))?;
    let mut rep = ExportReport { out_dir: out_dir.to_path_buf(), ..Default::default() };
    let mut used: HashSet<PathBuf> = HashSet::new();
    let total = entries.len();
    for (n, e) in entries.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            rep.errors.push("Đã dừng theo yêu cầu".into());
            break;
        }
        progress(n, total);
        let d = e.verdict.decision;
        let wanted = match d {
            Decision::Selected => true,
            Decision::Review => opts.copy_review,
            Decision::Rejected(_) => opts.copy_rejected,
        };
        if !wanted {
            continue;
        }
        let folder = out_dir.join(sub_folder(d)).join(&e.rel_dir);
        for f in e.files {
            let name = f.file_name().unwrap_or_default();
            let mut dst = folder.join(name);
            // Two different files with the same name (e.g. two cameras both at IMG_0001):
            // add a suffix rather than overwrite.
            let mut k = 2;
            while used.contains(&dst) {
                let stem = f.file_stem().unwrap_or_default().to_string_lossy();
                let ext = f.extension().map(|x| format!(".{}", x.to_string_lossy())).unwrap_or_default();
                dst = folder.join(format!("{stem}_{k}{ext}"));
                k += 1;
            }
            used.insert(dst.clone());
            if opts.move_files && !dst.exists() {
                if let Some(d) = dst.parent() {
                    std::fs::create_dir_all(d)?;
                }
                // Same drive: a rename moves the file without touching a single byte.
                if let Ok(len) = std::fs::metadata(f).map(|m| m.len()) {
                    if std::fs::rename(f, &dst).is_ok() {
                        rep.moved += 1;
                        rep.bytes += len;
                        continue;
                    }
                }
            }
            match copy_verified(f, &dst) {
                Ok(r) => {
                    match r {
                        Some(b) => {
                            rep.copied += 1;
                            rep.bytes += b;
                        }
                        None => rep.skipped_same += 1,
                    }
                    // Only now, with a verified identical copy in place, remove the original.
                    if opts.move_files {
                            let _ = set_readonly(f, false);
                        match std::fs::remove_file(f) {
                            Ok(()) => rep.moved += 1,
                            Err(e) => rep.errors.push(format!("Đã chép nhưng không xoá được bản gốc {}: {}", f.display(), explain_io(&e))),
                        }
                    }
                }
                Err(err) => rep.errors.push(format!("{err:#}")),
            }
        }
    }
    progress(total, total);
    if opts.report {
        write_csv(entries, &out_dir.join("bao-cao.csv"))?;
        std::fs::write(out_dir.join("tong-ket.txt"), summary_text(entries, &rep))?;
    }
    Ok(rep)
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', ';']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn write_csv(entries: &[Entry], path: &Path) -> Result<()> {
    let mut w = BufWriter::new(File::create(path)?);
    // BOM so Excel on Windows opens Vietnamese text correctly.
    w.write_all(b"\xEF\xBB\xBF")?;
    writeln!(w, "Tên file,Kết quả,Điểm,Phơi sáng,Độ nét,Bố cục,Màu sắc,Nhiễu,Người,Máy ảnh,Khoảnh khắc,Ghi chú,Gợi ý sửa,Sửa tay,File gốc")?;
    for e in entries {
        let v = e.verdict;
        let a = e.analysis;
        let sc = |f: fn(&Analysis) -> f32| a.map(|a| format!("{:.1}", f(a))).unwrap_or_default();
        let mut notes = v.notes.clone();
        if let Some(a) = a {
            notes.extend(a.people.notes.iter().cloned());
        }
        let row = [
            e.files.first().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            v.decision.label(),
            format!("{:.1}", v.score),
            sc(|a| a.scores.exposure),
            sc(|a| a.scores.sharp),
            sc(|a| a.scores.comp),
            sc(|a| a.scores.color),
            sc(|a| a.scores.noise),
            a.and_then(|a| a.scores.people).map(|p| format!("{p:.1}")).unwrap_or_default(),
            a.map(|a| a.camera.clone()).unwrap_or_default(),
            (v.moment + 1).to_string(),
            notes.join(" | "),
            v.fixes.join(" | "),
            if v.overridden { "có".into() } else { String::new() },
            e.files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(" + "),
        ];
        writeln!(w, "{}", row.iter().map(|s| csv_field(s)).collect::<Vec<_>>().join(","))?;
    }
    w.flush()?;
    Ok(())
}

pub fn summary_text(entries: &[Entry], rep: &ExportReport) -> String {
    let verdicts: Vec<Verdict> = entries.iter().map(|e| e.verdict.clone()).collect();
    let s = cull::summarize(&verdicts);
    let mut t = String::new();
    let pct = |n: usize| if s.total > 0 { n as f32 * 100.0 / s.total as f32 } else { 0.0 };
    t += "TỔNG KẾT BUỔI CHỤP\n==================\n\n";
    t += &format!("Tổng số ảnh: {}\n", s.total);
    t += &format!("Đã chọn:     {} ({:.0}%)\n", s.selected, pct(s.selected));
    t += &format!("Cần xem lại: {} ({:.0}%)\n", s.review, pct(s.review));
    t += &format!("Bị loại:     {} ({:.0}%)\n", s.rejected, pct(s.rejected));
    if s.moments > 1 {
        t += &format!("Số khoảnh khắc: {}\n", s.moments);
    }
    t += "\nLý do loại:\n";
    for (r, n) in &s.by_reason {
        t += &format!("  - {r}: {n}\n");
    }
    let mut cams: BTreeMap<String, (usize, usize, f32)> = BTreeMap::new();
    for e in entries {
        if let Some(a) = e.analysis {
            let c = cams.entry(a.camera.clone()).or_default();
            c.0 += 1;
            c.1 += e.verdict.decision.keep() as usize;
            c.2 += a.scores.sharp;
        }
    }
    if !cams.is_empty() {
        t += "\nTheo máy ảnh:\n";
        for (cam, (n, k, sh)) in &cams {
            t += &format!("  - {cam}: {n} ảnh, chọn {k}, độ nét trung bình {:.1}/10\n", sh / *n as f32);
        }
    }
    let manual = entries.iter().filter(|e| e.verdict.overridden).count();
    if manual > 0 {
        t += &format!("\nSơn đã sửa tay {manual} quyết định của AI ({:.0}% đồng ý với AI).\n", 100.0 - pct(manual));
    }
    if rep.moved > 0 {
        t += &format!("\nĐã chuyển {} file ({:.1} GB) sang thư mục này, ảnh gốc giữ nguyên chất lượng.\n", rep.moved, rep.bytes as f64 / 1e9);
    } else {
        t += &format!("\nĐã chép {} file ({:.1} GB), bỏ qua {} file đã có sẵn.\n", rep.copied, rep.bytes as f64 / 1e9, rep.skipped_same);
    }
    if !rep.errors.is_empty() {
        t += "\nLỖI:\n";
        for e in &rep.errors {
            t += &format!("  - {e}\n");
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cull::Reason;

    fn verdict(d: Decision) -> Verdict {
        Verdict {
            idx: 0,
            score: 8.0,
            rule_score: 8.0,
            learned: None,
            decision: d,
            ai_decision: d,
            overridden: false,
            notes: vec!["ghi chú, có dấu phẩy".into()],
            fixes: vec![],
            series: None,
            moment: 0,
            time: None,
        }
    }

    #[test]
    fn exports_into_decision_folders_without_touching_originals() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("Su-kien");
        std::fs::create_dir_all(&src).unwrap();
        let a = src.join("IMG_1.CR3");
        let a2 = src.join("IMG_1.JPG");
        let b = src.join("IMG_2.JPG");
        std::fs::write(&a, b"raw-bytes").unwrap();
        std::fs::write(&a2, b"jpeg-bytes").unwrap();
        std::fs::write(&b, b"other").unwrap();
        let fa = vec![a.clone(), a2.clone()];
        let fb = vec![b.clone()];
        let va = verdict(Decision::Selected);
        let vb = verdict(Decision::Rejected(Reason::ClosedEyes));
        let entries = vec![
            Entry { files: &fa, analysis: None, verdict: &va, rel_dir: PathBuf::new() },
            Entry { files: &fb, analysis: None, verdict: &vb, rel_dir: PathBuf::new() },
        ];
        let out = default_out_dir(&src);
        // Default: only the chosen shot, straight into «Su-kien/Su-kien_anh chon», no report files.
        let rep = export(&entries, &out, &ExportOptions::default(), &|_, _| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!(rep.copied, 2, "{:?}", rep.errors);
        assert!(out.ends_with("Su-kien/Su-kien_anh chon"));
        assert_eq!(std::fs::read(out.join("IMG_1.CR3")).unwrap(), b"raw-bytes");
        assert!(out.join("IMG_1.JPG").exists() && !out.join("IMG_2.JPG").exists());
        assert!(!out.join("bao-cao.csv").exists() && !out.join("tong-ket.txt").exists() && !out.join("Bi-loai").exists());
        let all = ExportOptions { copy_rejected: true, copy_review: true, report: true, move_files: false };
        let rep = export(&entries, &out, &all, &|_, _| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!((rep.copied, rep.skipped_same), (1, 2), "{:?}", rep.errors);
        assert_eq!(std::fs::read(out.join("Bi-loai/Nham-mat/IMG_2.JPG")).unwrap(), b"other");
        assert!(a.exists() && b.exists(), "originals stay");
        let csv = std::fs::read_to_string(out.join("bao-cao.csv")).unwrap();
        assert!(csv.starts_with('\u{feff}') && csv.contains("\"ghi chú, có dấu phẩy\""));
        // Running again skips identical files.
        let rep2 = export(&entries, &out, &all, &|_, _| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!((rep2.copied, rep2.skipped_same), (0, 3));
    }

    #[test]
    fn move_keeps_bytes_and_removes_originals() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("Su-kien");
        std::fs::create_dir_all(&src).unwrap();
        let raw: Vec<u8> = (0..200_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let a = src.join("DSC_1.NEF");
        std::fs::write(&a, &raw).unwrap();
        let fa = vec![a.clone()];
        let va = verdict(Decision::Selected);
        let entries = vec![Entry { files: &fa, analysis: None, verdict: &va, rel_dir: PathBuf::new() }];
        let out = dir.path().join("Xuat");
        let opts = ExportOptions { move_files: true, ..Default::default() };
        let rep = export(&entries, &out, &opts, &|_, _| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!(rep.moved, 1, "{:?}", rep.errors);
        assert!(!a.exists());
        assert_eq!(std::fs::read(out.join("DSC_1.NEF")).unwrap(), raw);
    }
}
