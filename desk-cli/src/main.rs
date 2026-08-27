// desk — 모션데스크 CLI (desk-core의 얇은 껍데기, 디버깅/파워유저용)
use clap::{Parser, Subcommand};
use desk_core::Desk;
use std::time::Duration;

const STEP_CM: f32 = 2.0; // up/down 1회당 이동량
const SCAN_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Parser)]
#[command(name = "desk", about = "데스커 모션데스크 BLE 제어")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// 현재 높이 출력
    Status,
    /// 2cm 상승
    Up,
    /// 2cm 하강
    Down,
    /// 정지
    Stop,
    /// 지정 높이(cm)로 이동
    To { cm: f32 },
}

#[tokio::main]
async fn main() -> Result<(), desk_core::Error> {
    let cli = Cli::parse();

    println!("스캔 중...");
    let desk = Desk::connect(SCAN_TIMEOUT).await?;
    let h = desk.height().await?;
    println!("현재 높이: {:.0} cm (raw={}, speed={})", h.cm, h.raw, h.speed);

    let moved = match cli.cmd {
        Cmd::Status => None,
        Cmd::Stop => {
            desk.stop().await?;
            println!("정지 명령 전송");
            None
        }
        Cmd::Up => Some(desk.move_by(STEP_CM).await?),
        Cmd::Down => Some(desk.move_by(-STEP_CM).await?),
        Cmd::To { cm } => {
            if !(desk_core::MIN_CM..=desk_core::MAX_CM).contains(&cm) {
                return Err(format!(
                    "목표 높이는 {}~{} cm 범위여야 함",
                    desk_core::MIN_CM,
                    desk_core::MAX_CM
                )
                .into());
            }
            Some(desk.move_to(cm).await?)
        }
    };

    if let Some(h) = moved {
        println!("이동 후 높이: {:.0} cm (raw={})", h.cm, h.raw);
    }

    desk.disconnect().await?;
    Ok(())
}
