# motion-desk-control — 계획

데스커 모션데스크 프리미엄(LINAK 컨트롤러)을 맥북에서 BLE로 제어하는 Rust 프로젝트.
DWilliames/idasen-desk-controller-mac (Swift, MIT)의 기능을 Rust로 재구현하되, 크로스 플랫폼(btleplug)을 지향한다.

## 검증된 사실 (2026-08-26 실기기 확인)

- 책상: 데스커 모션데스크 프리미엄, 전동 메커니즘 제조원 LINAK, 공식 앱 = Desk Control™
- BLE 이름 `DESK 5790` — IKEA IDÅSEN과 **완전히 동일한 순정 LINAK 프로토콜** (DPG 핸드셰이크 불필요)
- Python(bleak) / Rust(btleplug 0.11) 양쪽에서 스캔→연결→읽기→이동 명령 모두 성공
- 첫 연결만 스위치 ⌘버튼 2초(페어링 모드) 필요. 이후 OS 본딩이 유지되어 그냥 연결됨
- 동시 연결 1대 제한: 폰 Desk Control 앱이 물고 있으면 맥에서 연결 불가
- 스캔이 한 번에 안 잡힐 수 있음(광고 주기) → 재시도 루프 필수

## 프로토콜 레퍼런스

| 항목 | UUID (`99faXXXX-338a-1024-8a49-009c0215f78a`) | 내용 |
|---|---|---|
| 제어 write | `0002` | `0x4700` 상승, `0x4600` 하강, `0xFF00` 정지 (각 1회 ≈ 1초/1cm 남짓 이동) |
| 위치 read/notify | `0021` | `u16 LE` 위치(0.1mm, 최저점 기준) + `i16 LE` 속도 |
| 목표높이 write | `0031` | reference input (아직 미사용 — 이동 루프로 대체 중) |
| DPG | `0011` | 존재하나 사용 불필요 확인 |

- 실제 높이 = **630mm(최저높이) + raw/10 mm**
- "목표 높이 이동" = notify 받으며 up/down 반복 → 목표 ±0.5cm에서 stop (Swift 원본 방식, restore 스크립트로 검증)

## 단계별 계획

### 1단계 — 최소 동작 (현재, main.rs 단일 파일 ~70줄) ✅
- 스캔(이름 "DESK" 프리픽스) → 연결 → 현재 높이 출력

### 2단계 — CLI
- `desk status | up | down | stop | to <cm>` 서브커맨드 (clap)
- `to`: notify 구독 + 이동 루프 (오버슈트 보정: 방향별 0.5cm 선반영, 0.5초/0.5cm 최소 증분)
- 프리셋 저장: `desk save sit/stand`, `desk sit`, `desk stand` (설정 파일 ~/.config/motion-desk/config.toml)
- 스캔 재시도 내장, 종료 코드 정리

### 3단계 — 코어 분리
- 워크스페이스화: `desk-core`(라이브러리) + `desk-cli`(바이너리)

### 4단계 — 메뉴바 앱
- `tray-icon` + `muda`: 메뉴바에 현재 높이 표시(`↕ 77.4cm`), 메뉴로 앉기/서기/정지
- 구조: 메인 스레드 = UI 이벤트 루프, tokio 백그라운드 = BLE, 채널로 통신
- .app 번들(cargo-bundle): Info.plist에 `NSBluetoothAlwaysUsageDescription`, `LSUIElement=true`
- 로그인 자동 시작(LaunchAgent), 절전 해제 시 재연결

### 5단계 — 선택 기능
- AutoStand: 매시 자동 서기/앉기 (자리비움 감지는 macOS CGEventSource, OS별 분기)
- 크로스 플랫폼 빌드 확인 (Windows/Linux: 기기 식별자가 MAC 주소, OS 페어링 선행 필요)

## 참고 구현

- Swift 원본: https://github.com/DWilliames/idasen-desk-controller-mac (MIT)
- Rust CLI 선례: https://github.com/mitsuhiko/idasen-control , `idasen` crate
- Python 선례: https://github.com/aklajnert/idasen
