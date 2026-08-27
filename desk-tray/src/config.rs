// 설정 파일: ~/.config/motion-desk/config.toml
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Config {
    /// 앉기(①) 프리셋 높이 cm
    pub sit: Option<f32>,
    /// 서기(②) 프리셋 높이 cm
    pub stand: Option<f32>,
}

fn path() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("HOME 없음"))
        .join(".config/motion-desk/config.toml")
}

impl Config {
    pub fn load() -> Self {
        std::fs::read_to_string(path())
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
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
