// 데스커 모션데스크 프리미엄 (LINAK 컨트롤러) BLE 제어 코어 라이브러리.
//
// 프로토콜 (rhyst/linak-controller 참고, 실기기 검증):
// 1. 연결 후 DPG(0011) 핸드셰이크 — capabilities 읽기, user ID 읽고 첫 바이트를
//    1로 만들어 되쓰기. 이걸 하지 않으면 이후 모든 제어 쓰기가 조용히 무시된다.
// 2. 이동 전 제어(0002)에 wake-up(0xFE00) + 정지(0xFF00).
// 3. 이동은 reference input(0031)에 목표 raw(u16 LE)를 주기적으로 반복 쓰기 —
//    펌웨어가 알아서 가감속·정지하며, 갱신이 끊기면 안전상 자동 정지한다.
use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Manager, Peripheral};
use futures::StreamExt;
use std::time::Duration;
use tokio::time;
use uuid::Uuid;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

// LINAK reference output: 위치(u16 LE, 0.1mm, 최저점 기준) + 속도(i16 LE)
const POS_CHAR: Uuid = Uuid::from_u128(0x99fa0021_338a_1024_8a49_009c0215f78a);
// LINAK 제어 입력: 0xFE00 wake-up, 0xFF00 정지 (u16 LE)
const CTRL_CHAR: Uuid = Uuid::from_u128(0x99fa0002_338a_1024_8a49_009c0215f78a);
// LINAK reference input: 목표 위치(raw, u16 LE)
const REF_CHAR: Uuid = Uuid::from_u128(0x99fa0031_338a_1024_8a49_009c0215f78a);
// DPG 채널: [0x7F, cmd, 0x00] 쓰면 notify로 응답, [0x7F, cmd, 0x80, ...data]로 쓰기
const DPG_CHAR: Uuid = Uuid::from_u128(0x99fa0011_338a_1024_8a49_009c0215f78a);

const CMD_WAKEUP: [u8; 2] = [0xFE, 0x00];
const CMD_STOP: [u8; 2] = [0xFF, 0x00];
const DPG_GET_CAPABILITIES: u8 = 0x80;
const DPG_USER_ID: u8 = 0x86;

const BASE_MM: f32 = 630.0; // 모션데스크 프리미엄 최저 높이
pub const MIN_CM: f32 = 63.0;
pub const MAX_CM: f32 = 128.0; // 카탈로그 스펙 기준 추정 — 하드 리밋은 기기가 따로 가짐

/// reference input 갱신 주기 (linak-controller 기본값 부근)
const MOVE_TICK: Duration = Duration::from_millis(400);
/// 이동 루프 안전 상한 (전체 스트로크 왕복도 충분히 커버)
const MOVE_MAX_TICKS: u32 = 150;

#[derive(Debug, Clone, Copy)]
pub struct Height {
    pub cm: f32,
    pub raw: u16,
    /// 이동 중 부호 있는 속도, 정지 시 0
    pub speed: i16,
}

fn cm_to_raw(cm: f32) -> u16 {
    ((cm * 10.0 - BASE_MM) * 10.0).round().max(0.0) as u16
}

fn parse_height(data: &[u8]) -> Result<Height> {
    if data.len() < 4 {
        return Err("위치 데이터가 4바이트 미만".into());
    }
    let raw = u16::from_le_bytes([data[0], data[1]]);
    let speed = i16::from_le_bytes([data[2], data[3]]);
    Ok(Height {
        cm: (BASE_MM + raw as f32 / 10.0) / 10.0,
        raw,
        speed,
    })
}

pub struct Desk {
    peripheral: Peripheral,
    pos_char: btleplug::api::Characteristic,
    ctrl_char: btleplug::api::Characteristic,
    ref_char: btleplug::api::Characteristic,
}

impl Desk {
    /// 스캔(재시도 포함) → 연결 → 서비스 검색 → DPG 핸드셰이크까지 수행한다.
    /// `timeout` 동안 0.5초 간격으로 광고 수신을 확인한다.
    pub async fn connect(timeout: Duration) -> Result<Self> {
        let manager = Manager::new().await?;
        let central = manager
            .adapters()
            .await?
            .into_iter()
            .next()
            .ok_or("BLE 어댑터 없음")?;

        central.start_scan(ScanFilter::default()).await?;
        let mut found = None;
        let tries = (timeout.as_millis() / 200).max(1);
        'scan: for _ in 0..tries {
            time::sleep(Duration::from_millis(200)).await;
            for p in central.peripherals().await? {
                if let Some(props) = p.properties().await? {
                    if let Some(name) = props.local_name {
                        if name.to_uppercase().starts_with("DESK") {
                            found = Some(p);
                            break 'scan;
                        }
                    }
                }
            }
        }
        central.stop_scan().await?;
        let peripheral =
            found.ok_or("책상을 찾지 못함 (전원/거리 확인, 첫 연결이면 스위치 ⌘버튼 2초)")?;

        // 절전에서 깨어나는 중이면 연결 직후 끊기는 일이 흔함 → 몇 차례 재시도
        let mut last_err: Error = "연결 재시도 소진".into();
        for attempt in 0..3 {
            if attempt > 0 {
                let _ = peripheral.disconnect().await;
                time::sleep(Duration::from_millis(1000)).await;
            }
            match Self::setup(peripheral.clone()).await {
                Ok(desk) => return Ok(desk),
                Err(e) => {
                    eprintln!("연결 시도 {} 실패: {}", attempt + 1, e);
                    last_err = e;
                }
            }
        }
        Err(last_err)
    }

    /// 연결 → 서비스 검색 → 캐릭터리스틱 확인 → DPG 핸드셰이크
    async fn setup(peripheral: Peripheral) -> Result<Self> {
        peripheral.connect().await?;
        peripheral.discover_services().await?;

        let chars = peripheral.characteristics();
        let find = |uuid: Uuid, name: &'static str| {
            chars
                .iter()
                .find(|c| c.uuid == uuid)
                .cloned()
                .ok_or(format!("{} 캐릭터리스틱 없음", name))
        };
        let desk = Self {
            pos_char: find(POS_CHAR, "위치")?,
            ctrl_char: find(CTRL_CHAR, "제어")?,
            ref_char: find(REF_CHAR, "reference input")?,
            peripheral,
        };

        desk.dpg_handshake(find(DPG_CHAR, "DPG")?).await?;
        Ok(desk)
    }

    /// DPG 읽기: [0x7F, cmd, 0x00]을 쓰고 notify 응답을 기다린다. (0011 구독 상태 전제)
    async fn dpg_read(&self, dpg_char: &btleplug::api::Characteristic, cmd: u8) -> Result<Vec<u8>> {
        let mut notifs = self.peripheral.notifications().await?;
        self.peripheral
            .write(dpg_char, &[0x7F, cmd, 0x00], WriteType::WithResponse)
            .await?;
        let resp = time::timeout(Duration::from_secs(3), async {
            while let Some(n) = notifs.next().await {
                if n.uuid == DPG_CHAR {
                    return Some(n.value);
                }
            }
            None
        })
        .await
        .map_err(|_| "DPG 응답 타임아웃")?
        .ok_or("DPG notify 스트림 종료")?;
        Ok(resp)
    }

    /// DPG user ID 등록 — 이걸 해야 펌웨어가 이 클라이언트의 제어 쓰기를 받아들인다.
    async fn dpg_handshake(&self, dpg_char: btleplug::api::Characteristic) -> Result<()> {
        self.peripheral.subscribe(&dpg_char).await?;

        let _ = self.dpg_read(&dpg_char, DPG_GET_CAPABILITIES).await?;
        let resp = self.dpg_read(&dpg_char, DPG_USER_ID).await?;

        // 유효 응답이면 resp[0] == 0x01, 실데이터는 resp[2..]
        if resp.first() == Some(&0x01) && resp.len() > 2 {
            let mut user_id = resp[2..].to_vec();
            if user_id.first() != Some(&1) {
                user_id[0] = 1;
                let mut payload = vec![0x7F, DPG_USER_ID, 0x80];
                payload.extend_from_slice(&user_id);
                self.peripheral
                    .write(&dpg_char, &payload, WriteType::WithResponse)
                    .await?;
            }
        }

        self.peripheral.unsubscribe(&dpg_char).await?;
        Ok(())
    }

    pub async fn height(&self) -> Result<Height> {
        parse_height(&self.peripheral.read(&self.pos_char).await?)
    }

    /// 위치 notify 구독 — 물리 스위치 조작을 포함해 책상이 움직이는 동안
    /// 실시간 높이가 흘러나오는 스트림을 반환한다. 연결이 끊기면 스트림도 끝난다.
    pub async fn subscribe_height(&self) -> Result<impl futures::Stream<Item = Height> + Send> {
        self.peripheral.subscribe(&self.pos_char).await?;
        let notifs = self.peripheral.notifications().await?;
        Ok(notifs.filter_map(|n| async move {
            (n.uuid == POS_CHAR)
                .then(|| parse_height(&n.value).ok())
                .flatten()
        }))
    }

    /// 목표 높이(cm)까지 이동하고 최종 높이를 반환한다.
    /// reference input에 목표를 반복 기입하고, 펌웨어가 정지(speed=0)하면 끝낸다.
    pub async fn move_to(&self, target_cm: f32) -> Result<Height> {
        let target = cm_to_raw(target_cm.clamp(MIN_CM, MAX_CM));

        self.write_ctrl(&CMD_WAKEUP).await?;
        self.write_ctrl(&CMD_STOP).await?;

        for tick in 0..MOVE_MAX_TICKS {
            self.peripheral
                .write(&self.ref_char, &target.to_le_bytes(), WriteType::WithResponse)
                .await?;
            time::sleep(MOVE_TICK).await;
            let now = self.height().await?;
            // 첫 틱은 가속 전일 수 있어 건너뛰고, 이후 정지 = 도착(또는 장애물 감지)
            if tick >= 1 && now.speed == 0 {
                break;
            }
        }
        self.stop().await?;
        self.height().await
    }

    /// 현재 높이 기준 상대 이동 (+위 / -아래)
    pub async fn move_by(&self, delta_cm: f32) -> Result<Height> {
        let start = self.height().await?;
        self.move_to(start.cm + delta_cm).await
    }

    pub async fn stop(&self) -> Result<()> {
        self.write_ctrl(&CMD_STOP).await
    }

    pub async fn disconnect(self) -> Result<()> {
        self.peripheral.disconnect().await?;
        Ok(())
    }

    async fn write_ctrl(&self, cmd: &[u8; 2]) -> Result<()> {
        self.peripheral
            .write(&self.ctrl_char, cmd, WriteType::WithResponse)
            .await?;
        Ok(())
    }
}
