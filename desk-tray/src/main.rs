// desk-tray — 모션데스크 메뉴바 앱 (최소 구성)
// 구조: 메인 스레드 = tao 이벤트 루프(UI), 백그라운드 스레드 = tokio + BLE.
//       UI→BLE는 mpsc 채널, BLE→UI는 EventLoopProxy 사용자 이벤트.
use desk_core::Desk;
use std::sync::mpsc;
use std::time::Duration;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

const STEP_CM: f32 = 2.0;
const SCAN_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
enum UserEvent {
    Menu(MenuEvent),
    /// 메뉴바 타이틀 갱신 (예: "↕ 77.4cm", "↕ 연결 중...")
    Title(String),
}

enum DeskCmd {
    Up,
    Down,
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

    let menu = Menu::new();
    let up_item = MenuItem::new(format!("{}cm 상승", STEP_CM), true, None);
    let down_item = MenuItem::new(format!("{}cm 하강", STEP_CM), true, None);
    let stop_item = MenuItem::new("정지", true, None);
    let refresh_item = MenuItem::new("새로고침", true, None);
    let quit_item = MenuItem::new("종료", true, None);
    menu.append_items(&[
        &up_item,
        &down_item,
        &stop_item,
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
            Event::UserEvent(UserEvent::Menu(e)) => {
                let cmd = if e.id() == up_item.id() {
                    Some(DeskCmd::Up)
                } else if e.id() == down_item.id() {
                    Some(DeskCmd::Down)
                } else if e.id() == stop_item.id() {
                    Some(DeskCmd::Stop)
                } else if e.id() == refresh_item.id() {
                    Some(DeskCmd::Refresh)
                } else if e.id() == quit_item.id() {
                    // 프로세스 종료 시 BLE 연결이 끊기고 책상은 1초 내 자동 정지함
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

/// BLE 전담 스레드: 명령 채널을 소비하며 결과를 타이틀 이벤트로 보고한다.
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
                Ok(d) => desk = Some(d),
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
                DeskCmd::Up => d.move_by(STEP_CM).await.map(Some),
                DeskCmd::Down => d.move_by(-STEP_CM).await.map(Some),
                DeskCmd::Stop => d.stop().await.map(|_| None),
                DeskCmd::Refresh => d.height().await.map(Some),
            }
        });

        match result {
            Ok(Some(h)) => title(format!("↕ {:.0}cm", h.cm)),
            Ok(None) => {
                // 정지 직후 현재 높이 재표시
                if let Ok(h) = rt.block_on(d.height()) {
                    title(format!("↕ {:.0}cm", h.cm));
                }
            }
            Err(e) => {
                // 연결이 죽었을 가능성이 높음 → 버리고 다음 명령에서 재연결
                eprintln!("명령 실패: {}", e);
                desk = None;
                title("↕ 연결 안 됨".into());
            }
        }
    }
}
