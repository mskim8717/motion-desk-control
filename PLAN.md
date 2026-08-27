# motion-desk-control — 계획

데스커 모션데스크 프리미엄(LINAK 컨트롤러)을 맥북에서 BLE로 제어하는 Rust 프로젝트.
DWilliames/idasen-desk-controller-mac (Swift, MIT)의 기능을 Rust로 재구현하되, 크로스 플랫폼(btleplug)을 지향한다.

## 검증된 사실 (2026-08-26 실기기 확인)

- 책상: 데스커 모션데스크 프리미엄, 전동 메커니즘 제조원 LINAK, 공식 앱 = Desk Control™
- BLE 이름 `DESK 5790` — 순정 LINAK 프로토콜, 단 **DPG 핸드셰이크(user ID 등록) 필수** (2026-08-27 확인: 핸드셰이크 없으면 모든 제어 쓰기가 에러 없이 조용히 무시됨. "불필요"였던 기존 기록은 오류)
- Rust(btleplug 0.11)에서 스캔→연결→DPG 핸드셰이크→읽기→이동 전부 성공
- 첫 연결만 스위치 ⌘버튼 2초(페어링 모드) 필요. 이후 OS 본딩이 유지되어 그냥 연결됨
- 동시 연결 1대 제한: 폰 Desk Control 앱이 물고 있으면 맥에서 연결 불가
- 스캔이 한 번에 안 잡힐 수 있음(광고 주기) → 재시도 루프 필수

## 프로토콜 레퍼런스

| 항목 | UUID (`99faXXXX-338a-1024-8a49-009c0215f78a`) | 내용 |
|---|---|---|
| DPG | `0011` | **연결 직후 핸드셰이크 필수**: notify 구독 → `[0x7F, 0x80, 0x00]`(capabilities) → `[0x7F, 0x86, 0x00]`(user ID 읽기, 응답 `[0x01, _, data...]`) → data[0]≠1이면 1로 고쳐 `[0x7F, 0x86, 0x80, data...]`로 되쓰기 |
| 제어 write | `0002` | `0xFE00` wake-up, `0xFF00` 정지. (`0x4700`/`0x4600` up/down은 이 기기에서 무시됨 — IDÅSEN용 정보였음) |
| 위치 read/notify | `0021` | `u16 LE` 위치(0.1mm, 최저점 기준) + `i16 LE` 속도 |
| 목표높이 write | `0031` | **이동 수단**: 목표 raw(u16 LE)를 ~0.4초 주기로 반복 쓰기. 펌웨어가 가감속·도착 정지, 갱신 끊기면 자동 정지(데드맨) |

- 실제 높이 = **630mm(최저높이) + raw/10 mm**, raw = (cm×10 − 630)×10
- "목표 높이 이동" = wake-up → stop → `0031` 반복 쓰기, speed=0 되면 종료 (rhyst/linak-controller 방식, 2026-08-27 실기기 검증)

## 단계별 계획

### 1단계 — 최소 동작 ✅
- 스캔(이름 "DESK" 프리픽스) → 연결 → 현재 높이 출력

### 2단계 — CLI ✅ (프리셋 제외)
- `desk status | up | down | stop | to <cm>` 서브커맨드 (clap) — 완료, 실기기 검증
- `to`: `0031` reference input 반복 쓰기 방식 (위 프로토콜 참고)
- 프리셋 저장: `desk save sit/stand`, `desk sit`, `desk stand` (설정 파일 ~/.config/motion-desk/config.toml) — 미구현
- GUI 확정에 따라 CLI는 디버깅/파워유저용으로 유지

### 3단계 — 코어 분리 ✅
- 워크스페이스화: `desk-core`(라이브러리) + `desk-cli` + `desk-tray`

### 4단계 — 메뉴바 앱 (진행 중 — 최소 구성 완료)
- `tray-icon` + `muda`: 메뉴바에 현재 높이 표시(`↕ 78cm`, 정수), 메뉴로 상승/하강/정지/새로고침/종료 ✅
- 구조: 메인 스레드 = tao 이벤트 루프, 백그라운드 스레드 = tokio + BLE, mpsc/EventLoopProxy로 통신 ✅
- 앉기/서기 프리셋 메뉴 — 미구현
- .app 번들(cargo-bundle): Info.plist에 `NSBluetoothAlwaysUsageDescription`, `LSUIElement=true`
- 로그인 자동 시작(LaunchAgent), 절전 해제 시 재연결

### 5단계 — 선택 기능
- AutoStand: 매시 자동 서기/앉기 (자리비움 감지는 macOS CGEventSource, OS별 분기)
- 크로스 플랫폼 빌드 확인 (Windows/Linux: 기기 식별자가 MAC 주소, OS 페어링 선행 필요)

## 참고 구현

- Swift 원본: https://github.com/DWilliames/idasen-desk-controller-mac (MIT)
- Rust CLI 선례: https://github.com/mitsuhiko/idasen-control , `idasen` crate
- Python 선례: https://github.com/aklajnert/idasen
