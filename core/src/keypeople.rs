//! Key people Sơn marks himself (a bride, a speaker, the rector, a guest of honour): the app
//! remembers their faces and finds them in every photo by face fingerprint
//! ([`crate::faceid`]). Nobody is named or recognised unless Sơn marked them: the app only
//! says "this is the person you marked".

use crate::analyze::Analysis;
use crate::faceid::{similarity, SAME_PERSON};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Fingerprints kept per person (several angles and lights make the match surer).
const MAX_FACES: usize = 24;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeyPerson {
    pub id: u32,
    pub name: String,
    pub faces: Vec<Vec<i8>>,
    /// A photo thumbnail and the face box in it, to show the face in the app.
    #[serde(default)]
    pub thumb: Option<String>,
    #[serde(default)]
    pub bbox: [f32; 4],
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyBook {
    pub people: Vec<KeyPerson>,
    pub next: u32,
}

impl KeyBook {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// The marked person this face is, if any.
    pub fn who(&self, id: &[i8]) -> Option<u32> {
        if id.is_empty() {
            return None;
        }
        self.people
            .iter()
            .filter_map(|p| p.faces.iter().filter_map(|f| similarity(f, id)).fold(None, |m: Option<f32>, s| Some(m.map_or(s, |m| m.max(s)))).map(|s| (p.id, s)))
            .filter(|&(_, s)| s >= SAME_PERSON)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(p, _)| p)
    }

    /// Mark this face as a key person: added to `to` (or to whoever it already matches), or a
    /// new person. Returns the person's id.
    pub fn add(&mut self, id: Vec<i8>, name: &str, to: Option<u32>, thumb: Option<String>, bbox: [f32; 4]) -> Option<u32> {
        if id.len() != crate::faceid::DIM {
            return None;
        }
        let target = to.filter(|t| self.people.iter().any(|p| p.id == *t)).or_else(|| self.who(&id));
        let pid = match target {
            Some(t) => t,
            None => {
                self.next = self.next.max(self.people.iter().map(|p| p.id + 1).max().unwrap_or(0));
                let pid = self.next;
                self.next += 1;
                let name = if name.trim().is_empty() { format!("Nhân vật {}", pid + 1) } else { name.trim().to_string() };
                self.people.push(KeyPerson { id: pid, name, thumb, bbox, ..Default::default() });
                pid
            }
        };
        let p = self.people.iter_mut().find(|p| p.id == pid)?;
        if !name.trim().is_empty() {
            p.name = name.trim().to_string();
        }
        if !p.faces.iter().any(|f| similarity(f, &id).unwrap_or(0.0) > 0.97) {
            p.faces.push(id);
            if p.faces.len() > MAX_FACES {
                p.faces.remove(0);
            }
        }
        Some(pid)
    }

    pub fn remove(&mut self, pid: u32) -> bool {
        let n = self.people.len();
        self.people.retain(|p| p.id != pid);
        self.people.len() != n
    }

    pub fn rename(&mut self, pid: u32, name: &str) -> bool {
        match self.people.iter_mut().find(|p| p.id == pid) {
            Some(p) if !name.trim().is_empty() => {
                p.name = name.trim().to_string();
                true
            }
            _ => false,
        }
    }

    /// Fill `a.key_faces` from this list.
    pub fn mark(&self, a: &mut Analysis) {
        a.key_faces = if self.people.is_empty() {
            vec![]
        } else {
            a.people.faces.iter().enumerate().filter_map(|(i, f)| self.who(&f.id).map(|p| (i, p))).collect()
        };
        // A key person counts as a main face however small (the speaker seen from the back of
        // the hall), so their eyes and head are judged.
        let legacy = !a.people.faces.iter().any(|f| f.main_size);
        for (i, f) in a.people.faces.iter_mut().enumerate() {
            if legacy {
                f.main_size = f.main;
            }
            f.key = a.key_faces.iter().any(|&(k, _)| k == i);
            f.main = f.main_size || f.key;
        }
    }
}

/// Someone who appears again and again in the photos the client published (the rector, a
/// guest of honour): the app can take them as key people without Sơn marking them one by one.
#[derive(Debug, Clone, Serialize)]
pub struct Frequent {
    pub faces: Vec<Vec<i8>>,
    /// Published photos they are in.
    pub photos: usize,
    /// One photo of them and the face box in it.
    pub thumb: Option<String>,
    pub bbox: [f32; 4],
}

/// Fingerprints compared per person when grouping faces.
const GROUP_FACES: usize = 12;
/// At most this many people are taken from the published photos.
pub const FREQUENT_MAX: usize = 15;

/// People in at least `min_photos` of `photos` (path, analysis), most frequent first.
pub fn frequent(photos: &[(String, &Analysis)], min_photos: usize) -> Vec<Frequent> {
    struct Group {
        faces: Vec<Vec<i8>>,
        photos: std::collections::BTreeSet<usize>,
        thumb: Option<String>,
        bbox: [f32; 4],
        best: f32,
    }
    let mut groups: Vec<Group> = vec![];
    for (k, (path, a)) in photos.iter().enumerate() {
        for f in a.people.faces.iter().filter(|f| f.main && !f.small && f.id.len() == crate::faceid::DIM) {
            let size = f.bbox[2] * f.bbox[3];
            let hit = groups
                .iter_mut()
                .map(|g| (g.faces.iter().filter_map(|x| similarity(x, &f.id)).fold(0f32, f32::max), g))
                .filter(|(s, _)| *s >= SAME_PERSON)
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, g)| g);
            match hit {
                Some(g) => {
                    g.photos.insert(k);
                    if g.faces.len() < GROUP_FACES {
                        g.faces.push(f.id.clone());
                    }
                    // The biggest view of the face shows them best.
                    if size > g.best {
                        g.best = size;
                        g.thumb = Some(path.clone());
                        g.bbox = f.bbox;
                    }
                }
                None => groups.push(Group { faces: vec![f.id.clone()], photos: [k].into(), thumb: Some(path.clone()), bbox: f.bbox, best: size }),
            }
        }
    }
    let mut out: Vec<Frequent> = groups
        .into_iter()
        .filter(|g| g.photos.len() >= min_photos.max(2))
        .map(|g| Frequent { photos: g.photos.len(), faces: g.faces, thumb: g.thumb, bbox: g.bbox })
        .collect();
    out.sort_by(|a, b| b.photos.cmp(&a.photos));
    out.truncate(FREQUENT_MAX);
    out
}

impl KeyBook {
    /// Take the people found in published photos as key people: someone already in the list
    /// gets the new fingerprints, someone new is added under a plain name. Returns (new, known).
    pub fn learn_frequent(&mut self, people: &[Frequent]) -> (usize, usize) {
        let (mut new, mut known) = (0, 0);
        for p in people {
            let who = p.faces.iter().find_map(|f| self.who(f));
            let name = if who.is_some() { String::new() } else { format!("Hay có trong ảnh đã đăng {}", self.people.len() + 1) };
            let mut pid = who;
            for f in &p.faces {
                pid = self.add(f.clone(), if pid.is_none() { &name } else { "" }, pid, p.thumb.clone(), p.bbox).or(pid);
            }
            match (who, pid) {
                (Some(_), _) => known += 1,
                (None, Some(_)) => new += 1,
                _ => {}
            }
        }
        (new, known)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rand(seed: u64, n: usize) -> Vec<f32> {
        let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 40) as f32 / (1u64 << 24) as f32 - 0.5
            })
            .collect()
    }

    /// Person `k`, seen in photo `noise` (0: the marked one).
    fn person(k: u32, noise: u32) -> Vec<i8> {
        let base = rand(k as u64 + 1, crate::faceid::DIM);
        let n = rand(1000 + noise as u64, crate::faceid::DIM);
        let amt = if noise == 0 { 0.0 } else { 0.5 };
        crate::faceid::quantise(&base.iter().zip(&n).map(|(a, b)| a + amt * b).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn finds_the_marked_person_again() {
        let mut b = KeyBook::default();
        let rector = b.add(person(1, 0), "Thầy Hiệu trưởng", None, None, [0.0; 4]).unwrap();
        assert_eq!(b.who(&person(1, 5)), Some(rector), "same person, other photo");
        assert_eq!(b.who(&person(2, 0)), None);
        // Marking the same face again adds to that person, not a new one.
        assert_eq!(b.add(person(1, 9), "", None, None, [0.0; 4]), Some(rector));
        assert_eq!(b.people.len(), 1);
        assert_eq!(b.people[0].name, "Thầy Hiệu trưởng");
        let guest = b.add(person(2, 0), "", None, None, [0.0; 4]).unwrap();
        assert_ne!(guest, rector);
        assert_eq!(b.people[1].name, "Nhân vật 2");

        let mut a = Analysis::default();
        // Someone big in the front row, and the guest far away and small (not main by size).
        a.people.faces = vec![
            crate::faces::FaceInfo { id: person(3, 0), main: true, main_size: true, ..Default::default() },
            crate::faces::FaceInfo { id: person(2, 4), ..Default::default() },
        ];
        b.mark(&mut a);
        assert_eq!(a.key_faces, vec![(1, guest)]);
        assert!(a.people.faces[1].main && a.people.faces[1].key, "a key person is judged however small");
        assert!(b.remove(guest));
        b.mark(&mut a);
        assert!(a.key_faces.is_empty());
        assert!(!a.people.faces[1].main && a.people.faces[0].main);
    }

    #[test]
    fn people_often_in_published_photos_become_key_people() {
        // 20 published photos: the rector in 8, a guest in 3, a student once each.
        let face = |id: Vec<i8>, x: f32| crate::faces::FaceInfo { id, main: true, bbox: [x, 0.2, 0.1, 0.15], ..Default::default() };
        let mut photos = vec![];
        for k in 0..20u32 {
            let mut a = Analysis::default();
            a.people.faces.push(face(person(100 + k, 0), 0.1));
            if k % 5 < 2 {
                a.people.faces.push(face(person(1, k + 1), 0.4));
            }
            if k % 7 == 0 {
                a.people.faces.push(face(person(2, k + 1), 0.7));
            }
            photos.push((format!("/web/p{k}.jpg"), a));
        }
        let refs: Vec<(String, &Analysis)> = photos.iter().map(|(p, a)| (p.clone(), a)).collect();
        let f = frequent(&refs, 4);
        assert_eq!(f.len(), 1, "{:?}", f.iter().map(|x| x.photos).collect::<Vec<_>>());
        assert_eq!(f[0].photos, 8);
        assert_eq!(frequent(&refs, 3).len(), 2);
        let mut b = KeyBook::default();
        let marked = b.add(person(1, 0), "Thầy Hiệu trưởng", None, None, [0.0; 4]).unwrap();
        assert_eq!(b.learn_frequent(&frequent(&refs, 3)), (1, 1));
        assert_eq!(b.people.len(), 2);
        assert_eq!(b.people[0].name, "Thầy Hiệu trưởng", "his own name stays");
        assert!(b.people[0].faces.len() > 1);
        assert_eq!(b.who(&person(2, 50)), Some(b.people[1].id));
        assert_ne!(b.people[1].id, marked);
        assert!(b.people[1].name.starts_with("Hay có trong ảnh đã đăng"));
    }
}
