// 높이 이력 기록: ~/.local/share/motion-desk/history.csv (unix초,cm)
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn path() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("HOME 없음"))
        .join(".local/share/motion-desk/history.csv")
}

pub fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn append(cm: f32) {
    let p = path();
    let write = || -> std::io::Result<()> {
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&p)?;
        writeln!(f, "{},{:.1}", now_ts(), cm)
    };
    if let Err(e) = write() {
        eprintln!("이력 기록 실패 ({}): {}", p.display(), e);
    }
}

/// 최근 `secs`초 구간의 샘플. 구간 직전의 마지막 샘플도 하나 포함해
/// 차트가 구간 시작부터 높이를 알 수 있게 한다.
pub fn load_recent(secs: i64) -> Vec<(i64, f32)> {
    let since = now_ts() - secs;
    let mut before: Option<(i64, f32)> = None;
    let mut recent = Vec::new();
    if let Ok(text) = std::fs::read_to_string(path()) {
        for line in text.lines() {
            let mut it = line.splitn(2, ',');
            if let (Some(ts), Some(cm)) = (
                it.next().and_then(|s| s.parse::<i64>().ok()),
                it.next().and_then(|s| s.parse::<f32>().ok()),
            ) {
                if ts < since {
                    before = Some((ts, cm));
                } else {
                    recent.push((ts, cm));
                }
            }
        }
    }
    if let Some((_, cm)) = before {
        let mut v = vec![(since, cm)];
        v.extend(recent);
        v
    } else {
        recent
    }
}
