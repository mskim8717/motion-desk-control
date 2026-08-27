# MotionDesk

데스커 **모션데스크 프리미엄**(LINAK BLE 컨트롤러)을 맥에서 제어하는 메뉴바 앱 + CLI.

Rust로 작성했으며, 메뉴바에 현재 높이가 실시간으로 표시되고 저장해 둔 프리셋 높이로 한 번의 클릭으로 이동합니다.

```
↕ 78cm            ← 메뉴바 (실시간 갱신)
─────────────
① 앉기 (75cm)
② 서기 (110cm)
정지
─────────────
현재 높이를 ①로 저장
현재 높이를 ②로 저장
프리셋 초기화 ▸
─────────────
새로고침
종료
```

## 요구 사항

- Apple Silicon 맥 (M1 이후), macOS 12+
- 데스커 모션데스크 프리미엄 (다른 LINAK 기반 책상도 동작할 가능성이 높지만 미검증)

## 설치

[Releases](../../releases)에서 `MotionDesk.dmg`를 받아 앱을 Applications로 드래그하세요.

- 서명/공증이 없는 앱이므로 **첫 실행은 우클릭 → 열기**로 해야 합니다.
- 첫 실행 시 Bluetooth 권한을 허용하세요.
- **책상과의 첫 연결**이라면 스위치의 ⌘버튼을 2초간 눌러 페어링 모드로 만든 뒤 연결하세요. 이후에는 자동으로 연결됩니다.
- 폰의 Desk Control 앱이 책상에 연결되어 있으면 맥에서 연결할 수 없습니다 (동시 연결 1대 제한). 앱을 종료한 뒤 사용하세요.

## 소스 빌드

```bash
cargo run -p desk-tray          # 메뉴바 앱
cargo run -p desk-cli -- status # CLI
./scripts/bundle.sh             # MotionDesk.dmg 생성 (cargo-bundle 필요)
```

### CLI

```
desk status     현재 높이 출력
desk to <cm>    지정 높이로 이동 (63~128cm)
desk up / down  2cm 상승 / 하강
desk stop       정지
```

`cargo install --path desk-cli`로 설치하면 터미널 어디서든 `desk` 명령을 쓸 수 있습니다.

## 구조

| 크레이트 | 역할 |
|---|---|
| `desk-core` | BLE 로직 (btleplug) — 스캔, DPG 핸드셰이크, 이동, 위치 notify |
| `desk-cli` | `desk` 명령 (clap) |
| `desk-tray` | 메뉴바 앱 (tao + tray-icon) |

프리셋은 `~/.config/motion-desk/config.toml`에 저장됩니다.

## 프로토콜 메모

LINAK BLE 프로토콜의 핵심 (상세는 [PLAN.md](PLAN.md)):

- 연결 직후 **DPG 핸드셰이크**(user ID 등록, `99fa0011`)를 하지 않으면 이후 모든 제어 쓰기가 에러 없이 조용히 무시됩니다.
- 이동은 reference input(`99fa0031`)에 목표 위치를 ~0.4초 주기로 반복 기입하는 방식입니다. 갱신이 끊기면 펌웨어가 자동 정지하므로(데드맨) 앱이 죽어도 책상이 폭주하지 않습니다.
- 높이 = 630mm(최저) + raw/10 mm, 위치·속도는 `99fa0021`에서 read/notify.

## 참고 구현

- [rhyst/linak-controller](https://github.com/rhyst/linak-controller) — DPG 핸드셰이크·이동 방식의 출처
- [DWilliames/idasen-desk-controller-mac](https://github.com/DWilliames/idasen-desk-controller-mac) — 메뉴바 UX 참고

## 고지

이 프로젝트는 개인이 만든 **비공식** 소프트웨어로, 데스커(퍼시스그룹)·LINAK·기타 어떤 제조사와도 무관합니다. 제품명은 호환 대상을 설명하기 위해서만 사용했습니다. 전동 책상을 움직이는 소프트웨어이므로 사용에 따른 책임은 사용자에게 있으며, [MIT 라이선스](LICENSE)에 따라 어떠한 보증도 없이 제공됩니다.
