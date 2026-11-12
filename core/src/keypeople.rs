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
        a.people.faces = vec![crate::faces::FaceInfo { id: person(3, 0), ..Default::default() }, crate::faces::FaceInfo { id: person(2, 4), ..Default::default() }];
        b.mark(&mut a);
        assert_eq!(a.key_faces, vec![(1, guest)]);
        assert!(b.remove(guest));
        b.mark(&mut a);
        assert!(a.key_faces.is_empty());
    }
}
