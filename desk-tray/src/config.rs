// 설정 파일: ~/.config/motion-desk/config.toml
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const FAV_SLOTS: usize = 3;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Config {
    /// 즐겨찾기 높이(cm) 슬롯
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fav1: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fav2: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fav3: Option<f32>,
    /// 구버전 필드 — 읽기만 하고 favs로 이관
    #[serde(skip_serializing, default)]
    sit: Option<f32>,
    #[serde(skip_serializing, default)]
    stand: Option<f32>,
}

fn path() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("HOME 없음"))
        .join(".config/motion-desk/config.toml")
}

impl Config {
    pub fn load() -> Self {
        let mut cfg: Self = std::fs::read_to_string(path())
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default();
        // 구버전(sit/stand) → 즐겨찾기 이관
        if cfg.fav1.is_none() && cfg.fav2.is_none() && cfg.fav3.is_none() {
            cfg.fav1 = cfg.sit;
            cfg.fav2 = cfg.stand;
        }
        cfg.sit = None;
        cfg.stand = None;
        cfg
    }

    pub fn favs(&self) -> [Option<f32>; FAV_SLOTS] {
        [self.fav1, self.fav2, self.fav3]
    }

    pub fn set_fav(&mut self, i: usize, cm: f32) {
        match i {
            0 => self.fav1 = Some(cm),
            1 => self.fav2 = Some(cm),
            2 => self.fav3 = Some(cm),
            _ => {}
        }
    }

    pub fn save(&self) {
        let p = path();
        let write = || -> std::io::Result<()> {
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&p, toml::to_string(self).expect("config 직렬화 실패"))
        };
        if let Err(e) = write() {
            eprintln!("설정 저장 실패 ({}): {}", p.display(), e);
        }
    }
}
