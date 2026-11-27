//! Photos the client published (their website, Facebook albums saved to a folder): the photos
//! that made it, the standard Sơn delivers to. The app collects them from a website itself: it
//! walks the site's pages from the link given, keeps the articles of the last years, and saves
//! the large photos in them (logos, banners and pictures repeated on every page are left out).

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// What a website walk found.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Crawl {
    pub site: String,
    /// Folder the photos were saved into.
    pub dir: PathBuf,
    pub pages: usize,
    /// Pages left out because they are older than asked.
    pub old_pages: usize,
    /// Photos saved now, and photos already there from an earlier walk.
    pub saved: usize,
    pub had: usize,
    /// Pictures too small (icons, thumbnails) or repeated on many pages (logos, banners).
    pub skipped: usize,
    pub errors: usize,
}

/// Limits of one walk.
#[derive(Debug, Clone)]
pub struct Limits {
    pub max_pages: usize,
    pub max_photos: usize,
    /// Smallest long side (pixels) for a published photo; smaller pictures are thumbnails.
    pub min_side: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_pages: 1500, max_photos: 3000, min_side: 700 }
    }
}

/// A web address cut in its parts (enough for links found in pages).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Url {
    pub scheme: String,
    pub host: String,
    /// Path and query, starting with '/'.
    pub path: String,
}

impl Url {
    pub fn parse(s: &str) -> Option<Url> {
        let s = s.trim();
        let (scheme, rest) = s.split_once("://")?;
        let scheme = scheme.to_ascii_lowercase();
        if scheme != "http" && scheme != "https" {
            return None;
        }
        let rest = rest.split('#').next().unwrap_or("");
        let (host, path) = match rest.find(['/', '?']) {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        if host.is_empty() {
            return None;
        }
        let path = if path.starts_with('?') { format!("/{path}") } else { path.to_string() };
        Some(Url { scheme, host: host.to_ascii_lowercase(), path })
    }

    pub fn text(&self) -> String {
        format!("{}://{}{}", self.scheme, self.host, self.path)
    }

    /// The site, without "www." (www.hanu.vn and hanu.vn are one site).
    pub fn site(&self) -> &str {
        let h = self.host.split(':').next().unwrap_or(&self.host);
        h.strip_prefix("www.").unwrap_or(h)
    }

    /// A link found on this page, made absolute. `None` for mail, phone, scripts, anchors.
    pub fn join(&self, href: &str) -> Option<Url> {
        let h = decode_entities(href.trim());
        if h.is_empty() || h.starts_with('#') {
            return None;
        }
        let low = h.to_ascii_lowercase();
        if low.starts_with("http://") || low.starts_with("https://") {
            return Url::parse(&h);
        }
        if low.contains(':') && !low.starts_with('/') && low.find(':') < low.find('/').or(Some(usize::MAX)) {
            return None; // mailto:, tel:, javascript:, data:
        }
        if let Some(r) = h.strip_prefix("//") {
            return Url::parse(&format!("{}://{r}", self.scheme));
        }
        let h = h.split('#').next().unwrap_or("").to_string();
        let path = if h.starts_with('/') {
            h
        } else if h.starts_with('?') {
            format!("{}{h}", self.path.split('?').next().unwrap_or("/"))
        } else {
            let dir = self.path.split('?').next().unwrap_or("/");
            let dir = &dir[..dir.rfind('/').map(|i| i + 1).unwrap_or(1)];
            format!("{dir}{h}")
        };
        Some(Url { scheme: self.scheme.clone(), host: self.host.clone(), path: normalize(&path) })
    }

    fn ext(&self) -> String {
        let p = self.path.split('?').next().unwrap_or("");
        let last = p.rsplit('/').next().unwrap_or("");
        last.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
    }
}

/// Resolve "." and ".." in a path (the query is left alone).
fn normalize(p: &str) -> String {
    let (path, query) = match p.split_once('?') {
        Some((a, b)) => (a, Some(b)),
        None => (p, None),
    };
    let mut out: Vec<&str> = vec![];
    for seg in path.split('/').skip(1) {
        match seg {
            "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    let mut s = format!("/{}", out.join("/"));
    if let Some(q) = query {
        s.push('?');
        s.push_str(q);
    }
    s
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&").replace("&#38;", "&").replace("&quot;", "\"").replace("&#x2F;", "/").replace("&#47;", "/")
}

/// One HTML tag: its name and attributes (lower-case names).
struct Tag {
    name: String,
    attrs: HashMap<String, String>,
}

/// Every opening tag of a page, read loosely (real pages are rarely clean HTML).
fn tags(html: &str) -> Vec<Tag> {
    let b = html.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while let Some(off) = html[i..].find('<') {
        i += off + 1;
        if html[i..].starts_with("!--") {
            i = html[i..].find("-->").map(|k| i + k + 3).unwrap_or(b.len());
            continue;
        }
        let start = i;
        while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'-') {
            i += 1;
        }
        if i == start {
            continue;
        }
        let name = html[start..i].to_ascii_lowercase();
        if name == "script" || name == "style" {
            let close = format!("</{name}");
            let low_rest = html[i..].to_ascii_lowercase();
            i = low_rest.find(&close).map(|k| i + k).unwrap_or(b.len());
            continue;
        }
        let mut attrs = HashMap::new();
        loop {
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= b.len() || b[i] == b'>' || (b[i] == b'/' && b.get(i + 1) == Some(&b'>')) {
                break;
            }
            let ks = i;
            while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'=' && b[i] != b'>' && b[i] != b'/' {
                i += 1;
            }
            let key = html[ks..i].to_ascii_lowercase();
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            let mut val = String::new();
            if i < b.len() && b[i] == b'=' {
                i += 1;
                while i < b.len() && b[i].is_ascii_whitespace() {
                    i += 1;
                }
                if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                    let q = b[i];
                    i += 1;
                    let vs = i;
                    while i < b.len() && b[i] != q {
                        i += 1;
                    }
                    val = html[vs..i.min(b.len())].to_string();
                    i = (i + 1).min(b.len());
                } else {
                    let vs = i;
                    while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' {
                        i += 1;
                    }
                    val = html[vs..i].to_string();
                }
            } else if i == ks {
                i += 1;
            }
            if !key.is_empty() {
                attrs.entry(key).or_insert(val);
            }
        }
        out.push(Tag { name, attrs });
    }
    out
}

/// Days since 1970 of a calendar date.
fn days(y: i64, m: i64, d: i64) -> Option<i64> {
    if !(1990..=2100).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

fn num(s: &[u8]) -> Option<i64> {
    std::str::from_utf8(s).ok()?.parse().ok()
}

/// Dates written in a text: "26/11/2026", "26-11-2026", "2026-11-26", "2026/11/26", as days.
fn dates_in(text: &str) -> Vec<i64> {
    let b = text.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit() || (i > 0 && b[i - 1].is_ascii_digit()) {
            i += 1;
            continue;
        }
        let run = |j: usize| (j..b.len()).take_while(|&k| b[k].is_ascii_digit()).count();
        let n1 = run(i);
        let sep = b.get(i + n1).copied();
        if matches!(sep, Some(b'/') | Some(b'-') | Some(b'.')) {
            let j = i + n1 + 1;
            let n2 = run(j);
            if b.get(j + n2).copied() == sep {
                let k = j + n2 + 1;
                let n3 = run(k);
                let (a, m, c) = (num(&b[i..i + n1]), num(&b[j..j + n2]), num(&b[k..k + n3]));
                if let (Some(a), Some(m), Some(c)) = (a, m, c) {
                    let d = match (n1, n2, n3) {
                        (1..=2, 1..=2, 4) => days(c, m, a),
                        (4, 1..=2, 1..=2) => days(a, m, c),
                        _ => None,
                    };
                    if let Some(d) = d {
                        out.push(d);
                    }
                }
                i = k + n3;
                continue;
            }
        }
        i += n1.max(1);
    }
    out
}

/// "…/2025/11/…" in an address (uploads folders are dated): the first of that month.
fn url_month(path: &str) -> Option<i64> {
    let parts: Vec<&str> = path.split(['/', '?', '&', '=']).collect();
    parts.windows(2).find_map(|w| {
        let y: i64 = (w[0].len() == 4).then(|| w[0].parse().ok()).flatten()?;
        let m: i64 = (w[1].len() == 2).then(|| w[1].parse().ok()).flatten()?;
        days(y, m, 1)
    })
}

/// When a page was published, as days since 1970: its metadata first, then a date written in
/// it (other than `today`: many sites print today's date in the header).
fn page_date(tags: &[Tag], html: &str, url: &Url, today: i64) -> Option<i64> {
    for t in tags {
        let key = t.attrs.get("property").or_else(|| t.attrs.get("itemprop")).or_else(|| t.attrs.get("name")).map(|s| s.to_ascii_lowercase());
        if t.name == "meta" && key.as_deref().is_some_and(|k| k.contains("published") || k == "datepublished" || k == "date") {
            if let Some(d) = t.attrs.get("content").and_then(|c| dates_in(c).into_iter().next()) {
                return Some(d);
            }
        }
        if t.name == "time" {
            if let Some(d) = t.attrs.get("datetime").and_then(|c| dates_in(c).into_iter().next()) {
                return Some(d);
            }
        }
    }
    let body = html.to_ascii_lowercase().find("<body").map(|i| &html[i..]).unwrap_or(html);
    dates_in(body).into_iter().find(|&d| (d - today).abs() > 1).or_else(|| url_month(&url.path))
}

/// Links and pictures of a page.
fn links_and_pictures(tags: &[Tag], page: &Url) -> (Vec<Url>, Vec<Url>) {
    let mut links = vec![];
    let mut pics = vec![];
    for t in tags {
        match t.name.as_str() {
            "a" => {
                if let Some(u) = t.attrs.get("href").and_then(|h| page.join(h)) {
                    links.push(u);
                }
            }
            "img" | "source" => {
                // Lazy-loading pages keep the real picture in a data- attribute; srcset: the largest.
                let best_set = |v: &str| {
                    v.split(',')
                        .filter_map(|c| {
                            let mut it = c.split_whitespace();
                            let u = it.next()?;
                            let w: u32 = it.next().and_then(|w| w.trim_end_matches(['w', 'x']).parse().ok()).unwrap_or(1);
                            Some((w, u.to_string()))
                        })
                        .max_by_key(|x| x.0)
                        .map(|x| x.1)
                };
                let src = ["data-original", "data-src", "data-lazy-src", "data-full", "data-large"]
                    .iter()
                    .find_map(|k| t.attrs.get(*k).filter(|v| !v.is_empty()).cloned())
                    .or_else(|| t.attrs.get("data-srcset").or_else(|| t.attrs.get("srcset")).and_then(|v| best_set(v)))
                    .or_else(|| t.attrs.get("src").cloned());
                if let Some(u) = src.and_then(|s| page.join(&s)) {
                    pics.push(u);
                }
            }
            "meta" => {
                let key = t.attrs.get("property").or_else(|| t.attrs.get("name")).map(|s| s.to_ascii_lowercase());
                if key.as_deref() == Some("og:image") {
                    if let Some(u) = t.attrs.get("content").and_then(|c| page.join(c)) {
                        pics.push(u);
                    }
                }
            }
            _ => {}
        }
    }
    (links, pics)
}

/// File types that are not pages.
const NOT_PAGES: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "svg", "bmp", "ico", "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "zip", "rar", "mp4", "mp3", "avi", "mov", "css", "js", "xml", "json", "txt",
];

/// Pictures that are site furniture, by name.
fn furniture(u: &Url) -> bool {
    let p = u.path.to_ascii_lowercase();
    matches!(u.ext().as_str(), "svg" | "gif" | "ico" | "bmp")
        || ["logo", "icon", "banner", "avatar", "sprite", "button", "favicon", "placeholder", "loading", "qr-code", "qrcode"].iter().any(|w| p.contains(w))
}

/// Paths robots.txt asks robots to stay out of (the part for every robot).
fn disallowed(robots: &str) -> Vec<String> {
    let mut out = vec![];
    let mut all = false;
    for line in robots.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((k, v)) = line.split_once(':') else { continue };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
        if k == "user-agent" {
            all = v == "*";
        } else if k == "disallow" && all && !v.is_empty() {
            out.push(v.to_string());
        }
    }
    out
}

const AGENT: &str = "SonVuRawPick/1.0 (thu thap anh da dang de hoc phong cach chon anh)";
const MAX_PAGE: u64 = 4 << 20;
const MAX_PHOTO: u64 = 25 << 20;
/// A picture on more pages than this is part of the site, not of an article.
const MAX_PAGES_PER_PICTURE: usize = 3;

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout_read(Duration::from_secs(30)).user_agent(AGENT).redirects(5).build()
}

fn get(agent: &ureq::Agent, u: &Url, max: u64) -> Result<(String, Vec<u8>)> {
    let r = agent.get(&u.text()).call().with_context(|| format!("Không tải được {}", u.text()))?;
    let ctype = r.content_type().to_ascii_lowercase();
    let mut body = vec![];
    r.into_reader().take(max).read_to_end(&mut body)?;
    Ok((ctype, body))
}

/// Walk the site from `start` and save the photos of pages published since `since_days` (days
/// since 1970) into `out`. `step(done, total, what)` reports progress.
pub fn crawl(start: &str, since_days: i64, out: &Path, lim: &Limits, step: &dyn Fn(usize, usize, &str), cancel: &AtomicBool) -> Result<Crawl> {
    let start = Url::parse(start).or_else(|| Url::parse(&format!("https://{}", start.trim()))).context("Địa chỉ trang web không đúng (ví dụ: https://www.hanu.vn)")?;
    let site = start.site().to_string();
    std::fs::create_dir_all(out)?;
    let today = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64 / 86400).unwrap_or(0);
    let agent = agent();
    let robots = get(&agent, &Url { path: "/robots.txt".into(), ..start.clone() }, 1 << 20).map(|(_, b)| disallowed(&String::from_utf8_lossy(&b))).unwrap_or_default();
    let allowed = |u: &Url| !robots.iter().any(|d| u.path.starts_with(d.as_str()));

    let mut rep = Crawl { site: site.clone(), dir: out.to_path_buf(), ..Default::default() };
    let mut queue: VecDeque<Url> = VecDeque::from([start.clone()]);
    let mut seen: HashSet<String> = HashSet::from([start.text()]);
    // Picture -> pages it is on, and the newest date of those pages.
    let mut pics: HashMap<String, (Url, usize, Option<i64>)> = HashMap::new();
    let mut order: Vec<String> = vec![];
    let mut fetched = 0;
    while let Some(u) = queue.pop_front() {
        if cancel.load(Ordering::Relaxed) {
            bail!("Đã dừng");
        }
        if fetched >= lim.max_pages {
            break;
        }
        fetched += 1;
        step(fetched, lim.max_pages.min(fetched + queue.len()), &format!("Đang đọc trang web {site}"));
        let html = match get(&agent, &u, MAX_PAGE) {
            Ok((ct, b)) if ct.is_empty() || ct.contains("html") => String::from_utf8_lossy(&b).into_owned(),
            Ok(_) => continue,
            Err(_) => {
                rep.errors += 1;
                continue;
            }
        };
        rep.pages += 1;
        let t = tags(&html);
        let date = page_date(&t, &html, &u, today);
        // An old article, or a listing page that went back past the date asked: neither its
        // photos nor its links (older still) are wanted.
        let old = date.is_some_and(|d| d < since_days);
        if old && u != start {
            rep.old_pages += 1;
            continue;
        }
        let (links, found) = links_and_pictures(&t, &u);
        for p in found {
            if p.site() != site && !p.site().ends_with(&format!(".{site}")) && !p.host.contains("cdn") && !p.host.contains("fbcdn") {
                continue;
            }
            let e = pics.entry(p.text()).or_insert_with(|| {
                order.push(p.text());
                (p.clone(), 0, None)
            });
            e.1 += 1;
            e.2 = match (e.2, date) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            };
        }
        for l in links {
            if l.site() != site || NOT_PAGES.contains(&l.ext().as_str()) || !allowed(&l) {
                continue;
            }
            if seen.insert(l.text()) {
                queue.push_back(l);
            }
        }
    }

    // Article photos: not site furniture, on few pages, recent enough when dated.
    let wanted: Vec<&(Url, usize, Option<i64>)> = order
        .iter()
        .map(|k| &pics[k])
        .filter(|(u, n, d)| {
            let keep = !furniture(u) && *n <= MAX_PAGES_PER_PICTURE && d.or_else(|| url_month(&u.path)).is_none_or(|d| d >= since_days);
            keep
        })
        .collect();
    rep.skipped += pics.len() - wanted.len();
    let total = wanted.len().min(lim.max_photos);
    for (k, (u, _, _)) in wanted.into_iter().take(lim.max_photos).enumerate() {
        if cancel.load(Ordering::Relaxed) {
            bail!("Đã dừng");
        }
        step(k + 1, total, &format!("Đang tải ảnh đã đăng từ {site}"));
        let name = format!("{:08x}", crc32fast::hash(u.text().as_bytes()));
        if ["jpg", "png", "webp"].iter().any(|e| out.join(format!("{name}.{e}")).exists()) {
            rep.had += 1;
            continue;
        }
        let Ok((_, bytes)) = get(&agent, u, MAX_PHOTO) else {
            rep.errors += 1;
            continue;
        };
        let Ok(r) = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format() else {
            rep.skipped += 1;
            continue;
        };
        let ext = match r.format() {
            Some(image::ImageFormat::Jpeg) => "jpg",
            Some(image::ImageFormat::Png) => "png",
            Some(image::ImageFormat::WebP) => "webp",
            _ => {
                rep.skipped += 1;
                continue;
            }
        };
        match r.into_dimensions() {
            Ok((w, h)) if w.max(h) >= lim.min_side && w.min(h) >= lim.min_side / 2 => {}
            _ => {
                rep.skipped += 1;
                continue;
            }
        }
        std::fs::write(out.join(format!("{name}.{ext}")), &bytes)?;
        rep.saved += 1;
    }
    Ok(rep)
}

/// Folder name for a site's photos.
pub fn site_folder(start: &str) -> String {
    Url::parse(start).or_else(|| Url::parse(&format!("https://{}", start.trim()))).map(|u| u.site().replace([':', '/'], "_")).unwrap_or_else(|| "web".into())
}

/// Days since 1970, `years` back from now.
pub fn years_ago(years: u32) -> i64 {
    let today = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64 / 86400).unwrap_or(0);
    today - (years as f64 * 365.25) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    #[test]
    fn links_resolve_like_a_browser() {
        let p = Url::parse("https://www.hanu.vn/tin-tuc/su-kien/a.html?x=1#top").unwrap();
        assert_eq!(p.site(), "hanu.vn");
        assert_eq!(p.join("/uploads/2025/11/b.jpg").unwrap().text(), "https://www.hanu.vn/uploads/2025/11/b.jpg");
        assert_eq!(p.join("c.html").unwrap().text(), "https://www.hanu.vn/tin-tuc/su-kien/c.html");
        assert_eq!(p.join("../d.html").unwrap().text(), "https://www.hanu.vn/tin-tuc/d.html");
        assert_eq!(p.join("?page=2").unwrap().text(), "https://www.hanu.vn/tin-tuc/su-kien/a.html?page=2");
        assert_eq!(p.join("//cdn.hanu.vn/x.jpg").unwrap().text(), "https://cdn.hanu.vn/x.jpg");
        assert!(p.join("mailto:a@b.c").is_none() && p.join("javascript:void(0)").is_none() && p.join("#x").is_none());
        assert_eq!(p.join("/a?b=1&amp;c=2").unwrap().path, "/a?b=1&c=2");
    }

    #[test]
    fn dates_are_read_in_vietnamese_and_iso_forms() {
        let d = days(2026, 11, 26).unwrap();
        assert_eq!(dates_in("Cập nhật: 26/11/2026 08:15"), vec![d]);
        assert_eq!(dates_in("2026-11-26T08:15:00+07:00"), vec![d]);
        assert_eq!(dates_in("ngày 3-1-2025 và 2025/01/03"), vec![days(2025, 1, 3).unwrap(); 2]);
        assert!(dates_in("ĐT 024.3854.4338, phòng 12/34").is_empty());
        assert_eq!(url_month("/uploads/2024/05/x.jpg"), days(2024, 5, 1));
        assert_eq!(days(2000, 3, 1), Some(11017));
    }

    #[test]
    fn robots_rules_for_every_robot() {
        let r = "User-agent: Googlebot\nDisallow: /g\n\nUser-agent: *\nDisallow: /admin\nDisallow: # nothing\n";
        assert_eq!(disallowed(r), vec!["/admin".to_string()]);
    }

    fn jpeg(w: u32, h: u32, seed: u8) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x % 256) as u8 ^ seed, (y % 256) as u8, seed]));
        crate::metrics::encode_jpeg(&img, 85).unwrap()
    }

    /// A tiny website: a home page listing three articles (two recent, one old, plus a page 2),
    /// a logo and a thumbnail on every page.
    fn serve(pages: HashMap<String, (String, Vec<u8>)>) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for s in l.incoming() {
                let Ok(mut s) = s else { continue };
                let mut buf = [0u8; 4096];
                let n = std::io::Read::read(&mut s, &mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (ct, body) = pages.get(&path).cloned().unwrap_or(("text/html".into(), b"not found".to_vec()));
                let code = if pages.contains_key(&path) { "200 OK" } else { "404 Not Found" };
                let _ = write!(s, "HTTP/1.1 {code}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = s.write_all(&body);
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn walks_a_site_and_keeps_recent_article_photos() {
        let today = years_ago(0);
        let civil = |d: i64| {
            // back to y/m/d for writing pages
            let z = d + 719468;
            let era = z.div_euclid(146097);
            let doe = z - era * 146097;
            let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d = doy - (153 * mp + 2) / 5 + 1;
            let m = if mp < 10 { mp + 3 } else { mp - 9 };
            let y = yoe + era * 400 + (m <= 2) as i64;
            format!("{d:02}/{m:02}/{y}")
        };
        let chrome = |body: &str| format!(
            "<html><head><title>x</title></head><body><div class=top>Hôm nay {}</div><img src=\"/static/logo.png\"><img src=/static/side.jpg>{body}<a href=\"mailto:x@y.z\">mail</a></body></html>",
            civil(today)
        );
        let mut pages: HashMap<String, (String, Vec<u8>)> = HashMap::new();
        let html = |s: String| ("text/html; charset=utf-8".to_string(), s.into_bytes());
        pages.insert("/robots.txt".into(), ("text/plain".into(), b"User-agent: *\nDisallow: /admin\n".to_vec()));
        pages.insert("/".into(), html(chrome("<a href='/tin/1.html'>Lễ khai giảng</a> <a href=/tin/2.html>Hội thảo</a> <a href=\"tin/3.html\">Tin cũ</a> <a href='/?page=2'>2</a> <a href=/admin/x>admin</a> <a href=/files/a.pdf>pdf</a>")));
        pages.insert("/?page=2".into(), html(chrome("<a href='/tin/1.html'>again</a>")));
        pages.insert("/tin/1.html".into(), html(chrome(&format!("<p>Ngày đăng: {}</p><img data-src='/up/a1.jpg' src='/up/blank.gif'><img src='../up/a2.jpg' alt=x><img src=/up/small.jpg>", civil(today - 30)))));
        pages.insert("/tin/2.html".into(), html(chrome(&format!("<meta property=\"article:published_time\" content=\"{}\"><img srcset=\"/up/b1-300.jpg 300w, /up/b1.jpg 1600w\">", {
            let c = civil(today - 400);
            let p: Vec<&str> = c.split('/').collect();
            format!("{}-{}-{}T09:00:00+07:00", p[2], p[1], p[0])
        }))));
        pages.insert("/tin/3.html".into(), html(chrome(&format!("<p>{}</p><img src='/up/old.jpg'>", civil(today - 900)))));
        pages.insert("/admin/x".into(), html(chrome("<img src=/up/secret.jpg>")));
        for (k, w, h) in [("a1", 1600, 1067), ("a2", 1200, 800), ("b1", 1600, 1000), ("old", 1600, 1000), ("secret", 1600, 1000), ("small", 300, 200), ("side", 1000, 800)] {
            pages.insert(format!("/up/{k}.jpg"), ("image/jpeg".into(), jpeg(w, h, k.len() as u8 * 17)));
        }
        pages.insert("/static/side.jpg".into(), ("image/jpeg".into(), jpeg(1000, 800, 3)));
        pages.insert("/static/logo.png".into(), ("image/png".into(), b"x".to_vec()));
        let base = serve(pages);
        let out = std::env::temp_dir().join(format!("rawpick-web-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let rep = crawl(&base, years_ago(2), &out, &Limits::default(), &|_, _, _| {}, &AtomicBool::new(false)).unwrap();
        let mut saved: Vec<String> = std::fs::read_dir(&out).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        saved.sort();
        // a1, a2 (recent), b1 (400 days, largest of the srcset); not the old article, the admin
        // page, the small picture, the logo or the picture on every page.
        assert_eq!(rep.saved, 3, "{rep:?} {saved:?}");
        assert_eq!(rep.old_pages, 1, "{rep:?}");
        let want: Vec<String> = ["/up/a1.jpg", "/up/a2.jpg", "/up/b1.jpg"].iter().map(|p| format!("{:08x}.jpg", crc32fast::hash(format!("{base}{p}").as_bytes()))).collect();
        let mut want = want;
        want.sort();
        assert_eq!(saved, want);
        // Again: nothing new to download.
        let again = crawl(&base, years_ago(2), &out, &Limits::default(), &|_, _, _| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!((again.saved, again.had), (0, 3));
        let _ = std::fs::remove_dir_all(&out);
    }
}
