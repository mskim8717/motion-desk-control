// 데스커 모션데스크 프리미엄 (LINAK 컨트롤러) BLE 제어 — 1단계: 스캔 + 높이 읽기
// 실행: cargo run --release
use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::Manager;
use std::time::Duration;
use tokio::time;
use uuid::Uuid;

// LINAK reference output: 위치(u16 LE, 0.1mm, 최저점 기준) + 속도(i16 LE)
const POS_CHAR: Uuid = Uuid::from_u128(0x99fa0021_338a_1024_8a49_009c0215f78a);
const BASE_MM: f32 = 630.0; // 모션데스크 프리미엄 최저 높이

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = Manager::new().await?;
    let central = manager
        .adapters()
        .await?
        .into_iter()
        .next()
        .ok_or("BLE 어댑터 없음")?;

    println!("스캔 중...");
    central.start_scan(ScanFilter::default()).await?;

    // 광고 주기가 길어서 최대 30초까지 0.5초 간격으로 확인
    let mut desk = None;
    for _ in 0..60 {
        time::sleep(Duration::from_millis(500)).await;
        for p in central.peripherals().await? {
            if let Some(props) = p.properties().await? {
                if let Some(name) = props.local_name {
                    if name.to_uppercase().starts_with("DESK") {
                        println!("발견: {}", name);
                        desk = Some(p);
                    }
                }
            }
        }
        if desk.is_some() {
            break;
        }
    }
    central.stop_scan().await?;
    let desk = desk.ok_or("책상을 찾지 못함 (전원/거리 확인, 첫 연결이면 스위치 ⌘버튼 2초)")?;

    println!("연결 중...");
    desk.connect().await?;
    desk.discover_services().await?;

    let pos_char = desk
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == POS_CHAR)
        .ok_or("위치 캐릭터리스틱 없음")?;

    let data = desk.read(&pos_char).await?;
    let raw = u16::from_le_bytes([data[0], data[1]]);
    let speed = i16::from_le_bytes([data[2], data[3]]);
    println!(
        "현재 높이: {:.1} cm (raw={}, speed={})",
        (BASE_MM + raw as f32 / 10.0) / 10.0,
        raw,
        speed
    );

    desk.disconnect().await?;
    Ok(())
}
