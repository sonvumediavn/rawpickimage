//! What kind of event photo this is (overview, speaker, portrait, interaction, group,
//! activity, detail, moment), so an event is told from start to end and not only by its
//! prettiest frames.
//!
//! Judged from the faces (how many, how big, facing where, mouths) and a few ImageNet classes
//! the image model already sees (microphone, stage). A suggestion: Sơn's own sorting is what
//! counts. Nobody is identified by name unless Sơn marked that person himself.

use crate::analyze::Analysis;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scene {
    /// The whole hall or stage: shows how big the event is.
    Overview,
    /// Someone alone at the microphone or on stage.
    Speaker,
    /// One person, close: face, emotion.
    Portrait,
    /// Two to four people turned towards each other: handshakes, talks, gifts.
    Interaction,
    /// People posing for the camera together.
    Group,
    /// People busy with the programme.
    Activity,
    /// No one: lectern, logo, banner, documents, decoration.
    Detail,
    /// Laughter, cheering: emotion caught as it happened.
    Moment,
    #[default]
    Other,
}

impl Scene {
    pub const ALL: [Scene; 8] = [Scene::Overview, Scene::Speaker, Scene::Portrait, Scene::Interaction, Scene::Group, Scene::Activity, Scene::Detail, Scene::Moment];

    pub fn label(self) -> &'static str {
        match self {
            Scene::Overview => "Toàn cảnh",
            Scene::Speaker => "Phát biểu",
            Scene::Portrait => "Nhân vật",
            Scene::Interaction => "Tương tác",
            Scene::Group => "Tập thể",
            Scene::Activity => "Hoạt động",
            Scene::Detail => "Chi tiết",
            Scene::Moment => "Khoảnh khắc",
            Scene::Other => "Khác",
        }
    }

    /// Sub-folder name on export (no accents: safe on every drive and in Google Drive links).
    pub fn folder(self) -> &'static str {
        match self {
            Scene::Overview => "Toan-canh",
            Scene::Speaker => "Phat-bieu",
            Scene::Portrait => "Nhan-vat",
            Scene::Interaction => "Tuong-tac",
            Scene::Group => "Tap-the",
            Scene::Activity => "Hoat-dong",
            Scene::Detail => "Chi-tiet",
            Scene::Moment => "Khoanh-khac",
            Scene::Other => "Khac",
        }
    }
}

/// ImageNet classes (0-based) used here.
const MICROPHONE: u16 = 650;
const STAGE: [u16; 4] = [819, 854, 498, 598]; // stage, theater curtain, cinema, home theater

fn share(a: &Analysis, classes: &[u16]) -> f32 {
    a.objects.iter().filter(|(c, _)| classes.contains(c)).map(|&(_, p)| p as f32 / 100.0).sum()
}

pub fn classify(a: &Analysis) -> Scene {
    let faces = &a.people.faces;
    let main: Vec<_> = faces.iter().filter(|f| f.main && !f.small).collect();
    let widest = faces.iter().map(|f| f.bbox[2]).fold(0f32, f32::max);
    let stage = share(a, &STAGE);
    let mic = share(a, &[MICROPHONE]);
    let laughing = main.iter().any(|f| f.mouth_open >= 0.3);
    let facing = |f: &&crate::faces::FaceInfo| f.yaw.abs() < 0.3 && !f.turned && !f.down;
    if faces.is_empty() {
        return if stage >= 0.15 { Scene::Overview } else { Scene::Detail };
    }
    // People too small to matter one by one, or a crowd without anyone standing out.
    if (widest < 0.035 && faces.len() >= 3) || (faces.len() >= 8 && main.len() <= 2) {
        return Scene::Overview;
    }
    match main.len() {
        0 => Scene::Overview,
        1 => {
            if mic >= 0.05 || (stage >= 0.1 && widest < 0.12) {
                Scene::Speaker
            } else if laughing {
                Scene::Moment
            } else if widest >= 0.09 {
                Scene::Portrait
            } else {
                Scene::Activity
            }
        }
        2..=4 => {
            if laughing {
                Scene::Moment
            } else if main.iter().all(facing) {
                Scene::Group
            } else {
                Scene::Interaction
            }
        }
        n => {
            let posing = main.iter().filter(|f| facing(f)).count() as f32 / n as f32;
            if posing >= 0.7 {
                Scene::Group
            } else if laughing {
                Scene::Moment
            } else {
                Scene::Activity
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::faces::FaceInfo;

    fn with(faces: Vec<FaceInfo>, objects: Vec<(u16, u8)>) -> Analysis {
        let mut a = Analysis::default();
        a.people.faces = faces;
        a.objects = objects;
        a
    }

    fn face(x: f32, w: f32) -> FaceInfo {
        FaceInfo { bbox: [x, 0.3, w, w * 1.3], main: true, ..Default::default() }
    }

    #[test]
    fn sorts_the_usual_event_photos() {
        assert_eq!(classify(&with(vec![], vec![])), Scene::Detail);
        assert_eq!(classify(&with(vec![], vec![(819, 40)])), Scene::Overview);
        assert_eq!(classify(&with(vec![face(0.4, 0.06)], vec![(650, 20)])), Scene::Speaker);
        assert_eq!(classify(&with(vec![face(0.4, 0.2)], vec![])), Scene::Portrait);
        let crowd: Vec<FaceInfo> = (0..12).map(|k| FaceInfo { main: k < 2, ..face(k as f32 * 0.07, 0.02) }).collect();
        assert_eq!(classify(&with(crowd, vec![])), Scene::Overview);
        let row: Vec<FaceInfo> = (0..6).map(|k| face(0.1 + k as f32 * 0.13, 0.06)).collect();
        assert_eq!(classify(&with(row, vec![])), Scene::Group);
        let mut talk = vec![face(0.3, 0.1), face(0.6, 0.1)];
        talk[0].yaw = 0.5;
        talk[0].turned = true;
        assert_eq!(classify(&with(talk.clone(), vec![])), Scene::Interaction);
        talk[1].mouth_open = 0.4;
        assert_eq!(classify(&with(talk, vec![])), Scene::Moment);
    }
}
