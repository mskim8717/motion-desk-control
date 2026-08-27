// desk-tray — 모션데스크 메뉴바 앱
// 구조: 메인 스레드 = tao 이벤트 루프(UI), 백그라운드 스레드 = tokio + BLE.
//       UI→BLE는 mpsc 채널, BLE→UI는 EventLoopProxy 사용자 이벤트.
mod chart;
mod config;
mod history;

use config::Config;
use desk_core::Desk;
use futures::StreamExt;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tao::dpi::LogicalSize;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tao::window::{Window, WindowBuilder};
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

#[derive(Debug, Clone, Copy, PartialEq)]
enum ConnState {
    /// 연결 시도 중 — 조작 불가
    Connecting,
    /// 연결됨 — 전체 조작 가능
    Connected,
    /// 연결 실패/끊김 — 새로고침(재연결)만 가능
    Disconnected,
}

#[derive(Debug)]
enum UserEvent {
    Menu(MenuEvent),
    /// 메뉴바 타이틀 갱신 (예: "↕ 78cm", "↕ 이동 중...")
    Title(String),
    /// "현재 높이를 프리셋으로 저장" 완료 (슬롯, 측정된 높이)
    SlotSaved(Slot, f32),
    /// 연결 상태 변경 — 메뉴 활성/비활성 갱신
    Conn(ConnState),
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

    // 책상 조작 항목은 비활성으로 시작 — 연결되면 Conn(Connected) 이벤트가 활성화
    let sit_item = MenuItem::new(Slot::Sit.label(cfg.sit), false, None);
    let stand_item = MenuItem::new(Slot::Stand.label(cfg.stand), false, None);
    let stop_item = MenuItem::new("정지", false, None);
    let save_sit_item = MenuItem::new("현재 높이를 ①로 저장", false, None);
    let save_stand_item = MenuItem::new("현재 높이를 ②로 저장", false, None);
    let reset_sit_item = MenuItem::new(Slot::Sit.name(), cfg.sit.is_some(), None);
    let reset_stand_item = MenuItem::new(Slot::Stand.name(), cfg.stand.is_some(), None);
    let reset_menu = Submenu::with_items(
        "프리셋 초기화",
        true,
        &[&reset_sit_item, &reset_stand_item],
    )
    .expect("초기화 서브메뉴 구성 실패");
    let chart_item = MenuItem::new("사용 기록 보기", true, None);
    let refresh_item = MenuItem::new("새로고침", false, None);
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
        &chart_item,
        &refresh_item,
        &PredefinedMenuItem::separator(),
        &quit_item,
    ])
    .expect("메뉴 구성 실패");

    // macOS에서는 이벤트 루프 시작 후에 트레이 아이콘을 만들어야 함
    let mut tray: Option<TrayIcon> = None;
    // 사용 기록 창 (열려 있는 동안만 Some)
    let mut chart_win: Option<(Window, wry::WebView)> = None;

    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            // 닫기 요청 또는 포커스 상실(다른 곳 클릭) 시 팝오버 닫기
            Event::WindowEvent {
                event: WindowEvent::CloseRequested | WindowEvent::Focused(false),
                window_id,
                ..
            } => {
                if chart_win.as_ref().map(|(w, _)| w.id()) == Some(window_id) {
                    chart_win = None;
                }
            }
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
            Event::UserEvent(UserEvent::Conn(state)) => {
                let connected = state == ConnState::Connected;
                sit_item.set_enabled(connected && cfg.sit.is_some());
                stand_item.set_enabled(connected && cfg.stand.is_some());
                stop_item.set_enabled(connected);
                save_sit_item.set_enabled(connected);
                save_stand_item.set_enabled(connected);
                // 새로고침은 끊김 상태에서 재연결 버튼을 겸함 — 연결 시도 중에만 비활성
                refresh_item.set_enabled(state != ConnState::Connecting);
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
                } else if e.id() == chart_item.id() {
                    if chart_win.is_some() {
                        chart_win = None; // 이미 열려 있으면 토글로 닫기
                    } else {
                        // 서기 기준: 두 프리셋의 중간값, 없으면 90cm
                        let threshold = match (cfg.sit, cfg.stand) {
                            (Some(a), Some(b)) => (a + b) / 2.0,
                            _ => 90.0,
                        };
                        let html = chart::html(&history::load_recent(86400), threshold);
                        match open_chart_window(target, &html) {
                            Ok(win) => chart_win = Some(win),
                            Err(e) => eprintln!("사용 기록 창 생성 실패: {}", e),
                        }
                    }
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

fn open_chart_window(
    target: &tao::event_loop::EventLoopWindowTarget<UserEvent>,
    html: &str,
) -> Result<(Window, wry::WebView), Box<dyn std::error::Error>> {
    const W: f64 = 520.0;
    const H: f64 = 340.0;
    // 팝오버 스타일: 메뉴바 바로 아래 오른쪽에 배치 (트레이 아이콘 부근)
    let pos = target
        .primary_monitor()
        .map(|m| {
            let size = m.size().to_logical::<f64>(m.scale_factor());
            tao::dpi::LogicalPosition::new(size.width - W - 12.0, 34.0)
        })
        .unwrap_or(tao::dpi::LogicalPosition::new(0.0, 34.0));

    let window = WindowBuilder::new()
        .with_title("MotionDesk 사용 기록")
        .with_inner_size(LogicalSize::new(W, H))
        .with_position(pos)
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top(true)
        .with_resizable(false)
        .build(target)?;
    let webview = wry::WebViewBuilder::new()
        .with_transparent(true)
        .with_html(html)
        .build(&window)?;
    window.set_focus(); // 포커스를 받아야 포커스 상실 시 자동 닫힘이 동작
    Ok((window, webview))
}

/// BLE 전담 스레드: 명령 채널을 소비하며 결과를 사용자 이벤트로 보고한다.
fn ble_thread(cmd_rx: mpsc::Receiver<DeskCmd>, proxy: EventLoopProxy<UserEvent>) {
    let rt = tokio::runtime::Runtime::new().expect("tokio 런타임 생성 실패");
    let title = |s: String| {
        let _ = proxy.send_event(UserEvent::Title(s));
    };
    let conn = |s: ConnState| {
        let _ = proxy.send_event(UserEvent::Conn(s));
    };

    let mut desk: Option<Desk> = None;
    // 마지막으로 알려진 높이 — notify 태스크가 갱신, 하트비트 태스크가 기록
    let last_cm: Arc<Mutex<Option<f32>>> = Arc::new(Mutex::new(None));
    {
        // 5분마다 현재 높이를 이력에 기록 (움직임이 없어도 차트가 이어지도록)
        let last_cm = last_cm.clone();
        rt.spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(300));
            tick.tick().await; // 첫 틱은 즉시 발화하므로 건너뜀
            loop {
                tick.tick().await;
                if let Some(cm) = *last_cm.lock().unwrap() {
                    history::append(cm);
                }
            }
        });
    }

    while let Ok(cmd) = cmd_rx.recv() {
        // 연결이 없으면 먼저 연결 시도
        if desk.is_none() {
            title("↕ 연결 중...".into());
            conn(ConnState::Connecting);
            match rt.block_on(Desk::connect(SCAN_TIMEOUT)) {
                Ok(d) => {
                    // 연결 시점 높이를 이력에 기록
                    if let Ok(h) = rt.block_on(d.height()) {
                        history::append(h.cm);
                        *last_cm.lock().unwrap() = Some(h.cm);
                    }
                    // 위치 notify 구독: 물리 스위치로 움직여도 메뉴바 높이가 실시간 갱신됨
                    match rt.block_on(d.subscribe_height()) {
                        Ok(stream) => {
                            let p = proxy.clone();
                            let last_cm = last_cm.clone();
                            rt.spawn(async move {
                                let mut stream = Box::pin(stream);
                                let mut last_logged: Option<(i64, u16)> = None;
                                while let Some(h) = stream.next().await {
                                    let _ = p.send_event(UserEvent::Title(format!(
                                        "↕ {:.0}cm",
                                        h.cm
                                    )));
                                    *last_cm.lock().unwrap() = Some(h.cm);
                                    // 이동 중 이력: raw가 바뀌었을 때 초당 1회로 제한
                                    let ts = history::now_ts();
                                    let changed = last_logged
                                        .map(|(t, raw)| raw != h.raw && ts > t)
                                        .unwrap_or(true);
                                    if changed {
                                        history::append(h.cm);
                                        last_logged = Some((ts, h.raw));
                                    }
                                }
                            });
                        }
                        Err(e) => eprintln!("높이 알림 구독 실패: {}", e),
                    }
                    desk = Some(d);
                    conn(ConnState::Connected);
                }
                Err(e) => {
                    eprintln!("연결 실패: {}", e);
                    title("↕ 연결 안 됨".into());
                    conn(ConnState::Disconnected);
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
                conn(ConnState::Disconnected);
            }
        }
    }
}
