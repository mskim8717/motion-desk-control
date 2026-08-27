// desk-tray — 모션데스크 메뉴바 앱
// 구조: 메인 스레드 = tao 이벤트 루프(UI), 백그라운드 스레드 = tokio + BLE.
//       UI→BLE는 mpsc 채널, BLE→UI는 EventLoopProxy 사용자 이벤트.
mod config;

use config::Config;
use desk_core::Desk;
use futures::StreamExt;
use std::sync::mpsc;
use std::time::Duration;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder};

const SCAN_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq)]
enum Slot {
    Sit,
    Stand,
}

impl Slot {
    fn name(self) -> &'static str {
        match self {
            Slot::Sit => "① 앉기",
            Slot::Stand => "② 서기",
        }
    }

    fn label(self, cm: Option<f32>) -> String {
        match cm {
            Some(cm) => format!("{} ({:.0}cm)", self.name(), cm),
            None => format!("{} (미설정)", self.name()),
        }
    }
}

#[derive(Debug)]
enum UserEvent {
    Menu(MenuEvent),
    /// 메뉴바 타이틀 갱신 (예: "↕ 78cm", "↕ 이동 중...")
    Title(String),
    /// "현재 높이를 프리셋으로 저장" 완료 (슬롯, 측정된 높이)
    SlotSaved(Slot, f32),
}

enum DeskCmd {
    GoTo(f32),
    SaveSlot(Slot),
    Stop,
    Refresh,
}

fn main() {
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    event_loop.set_activation_policy(ActivationPolicy::Accessory); // 독 아이콘 숨김 (메뉴바 전용)

    // 메뉴 이벤트 → 이벤트 루프로 전달
    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let _ = proxy.send_event(UserEvent::Menu(e));
    }));

    // BLE 백그라운드 스레드
    let (cmd_tx, cmd_rx) = mpsc::channel::<DeskCmd>();
    let ble_proxy = event_loop.create_proxy();
    std::thread::spawn(move || ble_thread(cmd_rx, ble_proxy));
    let _ = cmd_tx.send(DeskCmd::Refresh); // 시작하자마자 연결 + 높이 표시

    let mut cfg = Config::load();

    let sit_item = MenuItem::new(Slot::Sit.label(cfg.sit), cfg.sit.is_some(), None);
    let stand_item = MenuItem::new(Slot::Stand.label(cfg.stand), cfg.stand.is_some(), None);
    let stop_item = MenuItem::new("정지", true, None);
    let save_sit_item = MenuItem::new("현재 높이를 ①로 저장", true, None);
    let save_stand_item = MenuItem::new("현재 높이를 ②로 저장", true, None);
    let reset_sit_item = MenuItem::new(Slot::Sit.name(), cfg.sit.is_some(), None);
    let reset_stand_item = MenuItem::new(Slot::Stand.name(), cfg.stand.is_some(), None);
    let reset_menu = Submenu::with_items(
        "프리셋 초기화",
        true,
        &[&reset_sit_item, &reset_stand_item],
    )
    .expect("초기화 서브메뉴 구성 실패");
    let refresh_item = MenuItem::new("새로고침", true, None);
    let quit_item = MenuItem::new("종료", true, None);

    let menu = Menu::new();
    menu.append_items(&[
        &sit_item,
        &stand_item,
        &stop_item,
        &PredefinedMenuItem::separator(),
        &save_sit_item,
        &save_stand_item,
        &reset_menu,
        &PredefinedMenuItem::separator(),
        &refresh_item,
        &PredefinedMenuItem::separator(),
        &quit_item,
    ])
    .expect("메뉴 구성 실패");

    // macOS에서는 이벤트 루프 시작 후에 트레이 아이콘을 만들어야 함
    let mut tray: Option<TrayIcon> = None;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(StartCause::Init) => {
                tray = Some(
                    TrayIconBuilder::new()
                        .with_title("↕ 연결 중...")
                        .with_menu(Box::new(menu.clone()))
                        .build()
                        .expect("트레이 아이콘 생성 실패"),
                );
            }
            Event::UserEvent(UserEvent::Title(title)) => {
                if let Some(t) = &tray {
                    t.set_title(Some(&title));
                }
            }
            Event::UserEvent(UserEvent::SlotSaved(slot, cm)) => {
                let (field, item, reset_item) = match slot {
                    Slot::Sit => (&mut cfg.sit, &sit_item, &reset_sit_item),
                    Slot::Stand => (&mut cfg.stand, &stand_item, &reset_stand_item),
                };
                *field = Some(cm);
                cfg.save();
                item.set_text(slot.label(Some(cm)));
                item.set_enabled(true);
                reset_item.set_enabled(true);
            }
            Event::UserEvent(UserEvent::Menu(e)) => {
                let cmd = if e.id() == sit_item.id() {
                    cfg.sit.map(DeskCmd::GoTo)
                } else if e.id() == stand_item.id() {
                    cfg.stand.map(DeskCmd::GoTo)
                } else if e.id() == stop_item.id() {
                    Some(DeskCmd::Stop)
                } else if e.id() == save_sit_item.id() {
                    Some(DeskCmd::SaveSlot(Slot::Sit))
                } else if e.id() == save_stand_item.id() {
                    Some(DeskCmd::SaveSlot(Slot::Stand))
                } else if e.id() == reset_sit_item.id() || e.id() == reset_stand_item.id() {
                    let slot = if e.id() == reset_sit_item.id() {
                        Slot::Sit
                    } else {
                        Slot::Stand
                    };
                    let (field, item, reset_item) = match slot {
                        Slot::Sit => (&mut cfg.sit, &sit_item, &reset_sit_item),
                        Slot::Stand => (&mut cfg.stand, &stand_item, &reset_stand_item),
                    };
                    *field = None;
                    cfg.save();
                    item.set_text(slot.label(None));
                    item.set_enabled(false);
                    reset_item.set_enabled(false);
                    None
                } else if e.id() == refresh_item.id() {
                    Some(DeskCmd::Refresh)
                } else if e.id() == quit_item.id() {
                    // 프로세스 종료 시 BLE 연결이 끊기고 책상은 자동 정지함 (데드맨)
                    *control_flow = ControlFlow::Exit;
                    None
                } else {
                    None
                };
                if let Some(cmd) = cmd {
                    let _ = cmd_tx.send(cmd);
                }
            }
            _ => {}
        }
    });
}

/// BLE 전담 스레드: 명령 채널을 소비하며 결과를 사용자 이벤트로 보고한다.
fn ble_thread(cmd_rx: mpsc::Receiver<DeskCmd>, proxy: EventLoopProxy<UserEvent>) {
    let rt = tokio::runtime::Runtime::new().expect("tokio 런타임 생성 실패");
    let title = |s: String| {
        let _ = proxy.send_event(UserEvent::Title(s));
    };

    let mut desk: Option<Desk> = None;

    while let Ok(cmd) = cmd_rx.recv() {
        // 연결이 없으면 먼저 연결 시도
        if desk.is_none() {
            title("↕ 연결 중...".into());
            match rt.block_on(Desk::connect(SCAN_TIMEOUT)) {
                Ok(d) => {
                    // 위치 notify 구독: 물리 스위치로 움직여도 메뉴바 높이가 실시간 갱신됨
                    match rt.block_on(d.subscribe_height()) {
                        Ok(stream) => {
                            let p = proxy.clone();
                            rt.spawn(async move {
                                let mut stream = Box::pin(stream);
                                while let Some(h) = stream.next().await {
                                    let _ = p.send_event(UserEvent::Title(format!(
                                        "↕ {:.0}cm",
                                        h.cm
                                    )));
                                }
                            });
                        }
                        Err(e) => eprintln!("높이 알림 구독 실패: {}", e),
                    }
                    desk = Some(d);
                }
                Err(e) => {
                    eprintln!("연결 실패: {}", e);
                    title("↕ 연결 안 됨".into());
                    continue;
                }
            }
        }
        let d = desk.as_ref().unwrap();

        let result = rt.block_on(async {
            match cmd {
                DeskCmd::GoTo(cm) => {
                    title(format!("↕ {:.0}cm로 이동 중...", cm));
                    d.move_to(cm).await.map(Some)
                }
                DeskCmd::Stop => d.stop().await.and(d.height().await).map(Some),
                DeskCmd::Refresh => d.height().await.map(Some),
                DeskCmd::SaveSlot(slot) => d.height().await.map(|h| {
                    let _ = proxy.send_event(UserEvent::SlotSaved(slot, h.cm));
                    Some(h)
                }),
            }
        });

        match result {
            Ok(Some(h)) => title(format!("↕ {:.0}cm", h.cm)),
            Ok(None) => {}
            Err(e) => {
                // 연결이 죽었을 가능성이 높음 → 버리고 다음 명령에서 재연결
                eprintln!("명령 실패: {}", e);
                desk = None;
                title("↕ 연결 안 됨".into());
            }
        }
    }
}
