// desk-tray — 모션데스크 메뉴바 앱
// 좌클릭: 통합 팝오버 패널 (높이 + 앉기/서기/정지 + 사용 기록 차트)
// 우클릭: 보조 메뉴 (프리셋 저장/초기화, 새로고침, 종료)
// 구조: 메인 스레드 = tao 이벤트 루프(UI), 백그라운드 스레드 = tokio + BLE.
mod config;
mod history;
mod panel;

use config::Config;
use desk_core::Desk;
use futures::StreamExt;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use std::time::{Duration, Instant};
use tao::dpi::{LogicalPosition, LogicalSize};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tao::window::{Window, WindowBuilder};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

const SCAN_TIMEOUT: Duration = Duration::from_secs(30);
const PANEL_W: f64 = 260.0;
const PANEL_H: f64 = 312.0;


#[derive(Debug, Clone, Copy, PartialEq)]
enum ConnState {
    /// 연결 시도 중 — 조작 불가
    Connecting,
    /// 연결됨 — 전체 조작 가능
    Connected,
    /// 연결 실패/끊김 — 새로고침(재연결)만 가능
    Disconnected,
}

impl ConnState {
    fn label(self) -> &'static str {
        match self {
            ConnState::Connecting => "연결 중...",
            ConnState::Connected => "연결됨",
            ConnState::Disconnected => "연결 안 됨",
        }
    }
}

#[derive(Debug)]
enum UserEvent {
    /// 트레이 아이콘 좌클릭 — 패널 토글
    TrayClick,
    /// 패널 버튼 (sit | stand | stop)
    Ipc(String),
    /// 메뉴바 타이틀 갱신 (예: "↕ 78cm", "↕ 이동 중...")
    Title(String),
    /// "현재 높이를 즐겨찾기로 저장" 완료 (슬롯 번호, 측정된 높이)
    SlotSaved(usize, f32),
    /// 연결 상태 변경 — 메뉴/패널 활성 갱신
    Conn(ConnState),
}

enum DeskCmd {
    GoTo(f32),
    SaveSlot(usize),
    Stop,
    Refresh,
}

fn main() {
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    event_loop.set_activation_policy(ActivationPolicy::Accessory); // 독 아이콘 숨김 (메뉴바 전용)

    // 트레이 이벤트 → 이벤트 루프로 전달 (좌클릭만, 우클릭은 무시)
    let proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Down,
            ..
        } = e
        {
            let _ = proxy.send_event(UserEvent::TrayClick);
        }
    }));

    // BLE 백그라운드 스레드
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<DeskCmd>();
    let ble_proxy = event_loop.create_proxy();
    std::thread::spawn(move || ble_thread(cmd_rx, ble_proxy));
    let _ = cmd_tx.send(DeskCmd::Refresh); // 시작하자마자 연결 + 높이 표시

    let mut cfg = Config::load();

    // macOS에서는 이벤트 루프 시작 후에 트레이 아이콘을 만들어야 함
    let mut tray: Option<TrayIcon> = None;
    // 팝오버 패널 (열려 있는 동안만 Some)
    let mut panel_win: Option<(Window, wry::WebView)> = None;
    // 포커스 상실로 패널이 닫힌 직후의 트레이 클릭은 "닫기" 의도로 보고 무시
    let mut panel_closed_at: Option<Instant> = None;
    let mut cur_title = String::from("↕ 연결 중...");
    let mut cur_conn = ConnState::Connecting;

    let ipc_proxy = event_loop.create_proxy();

    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(StartCause::Init) => {
                tray = Some(
                    TrayIconBuilder::new()
                        .with_title(&cur_title)
                        .build()
                        .expect("트레이 아이콘 생성 실패"),
                );
            }
            // 닫기 요청 또는 포커스 상실(다른 곳 클릭) 시 패널 닫기
            Event::WindowEvent {
                event: WindowEvent::CloseRequested | WindowEvent::Focused(false),
                window_id,
                ..
            } => {
                if panel_win.as_ref().map(|(w, _)| w.id()) == Some(window_id) {
                    panel_win = None;
                    panel_closed_at = Some(Instant::now());
                }
            }
            Event::UserEvent(UserEvent::TrayClick) => {
                if panel_win.is_some() {
                    panel_win = None;
                } else {
                    // 방금 포커스 상실로 닫혔다면 이 클릭은 닫기 의도였음 → 무시
                    let just_closed = panel_closed_at
                        .map(|t| t.elapsed() < Duration::from_millis(300))
                        .unwrap_or(false);
                    if !just_closed {
                        let state = panel::PanelState {
                            big: cur_title.trim_start_matches("↕ "),
                            connected: cur_conn == ConnState::Connected,
                            favs: cfg.favs(),
                            samples: &history::load_recent(86400),
                            threshold_cm: standing_threshold(&cfg),
                        };
                        match open_panel(target, tray.as_ref(), &ipc_proxy, &panel::html(&state))
                        {
                            Ok(win) => panel_win = Some(win),
                            Err(e) => eprintln!("패널 생성 실패: {}", e),
                        }
                    }
                }
            }
            Event::UserEvent(UserEvent::Ipc(msg)) => {
                let cmd = match msg.as_str() {
                    // ▲▼ 홀드: 리밋 방향으로 이동 시작, 버튼을 떼면 "stop"이 와서 중단
                    "up" => Some(DeskCmd::GoTo(desk_core::MAX_CM)),
                    "down" => Some(DeskCmd::GoTo(desk_core::MIN_CM)),
                    "stop" => Some(DeskCmd::Stop),
                    "refresh" => Some(DeskCmd::Refresh),
                    "quit" => {
                        // 프로세스 종료 시 BLE 연결이 끊기고 책상은 자동 정지함 (데드맨)
                        *control_flow = ControlFlow::Exit;
                        None
                    }
                    // "fav:N" 즐겨찾기로 이동, "save:N" 현재 높이를 슬롯 N에 저장
                    m => {
                        if let Some(i) = m.strip_prefix("fav:").and_then(|s| s.parse().ok()) {
                            cfg.favs().get::<usize>(i).copied().flatten().map(DeskCmd::GoTo)
                        } else if let Some(i) =
                            m.strip_prefix("save:").and_then(|s| s.parse::<usize>().ok())
                        {
                            (i < config::FAV_SLOTS).then_some(DeskCmd::SaveSlot(i))
                        } else {
                            None
                        }
                    }
                };
                if let Some(cmd) = cmd {
                    let _ = cmd_tx.send(cmd);
                }
            }
            Event::UserEvent(UserEvent::Title(title)) => {
                cur_title = title;
                if let Some(t) = &tray {
                    t.set_title(Some(&cur_title));
                }
                if let Some((_, wv)) = &panel_win {
                    let big = cur_title.trim_start_matches("↕ ").replace('\'', "");
                    let _ = wv.evaluate_script(&format!("setTitle('{}')", big));
                }
            }
            Event::UserEvent(UserEvent::Conn(state)) => {
                cur_conn = state;
                let connected = state == ConnState::Connected;
                if let Some((_, wv)) = &panel_win {
                    let _ = wv.evaluate_script(&format!(
                        "setConn({},'{}')",
                        connected,
                        state.label()
                    ));
                }
            }
            Event::UserEvent(UserEvent::SlotSaved(slot, cm)) => {
                cfg.set_fav(slot, cm);
                cfg.save();
                if let Some((_, wv)) = &panel_win {
                    let favs = cfg
                        .favs()
                        .iter()
                        .map(|f| f.map(|v| format!("{:.1}", v)).unwrap_or_else(|| "null".into()))
                        .collect::<Vec<_>>()
                        .join(",");
                    let _ = wv.evaluate_script(&format!("setFavs([{}])", favs));
                }
            }
            _ => {}
        }
    });
}

/// 서기 판정 기준: 즐겨찾기 최저·최고의 중간값, 2개 미만이면 90cm
fn standing_threshold(cfg: &Config) -> f32 {
    let set: Vec<f32> = cfg.favs().iter().flatten().copied().collect();
    if set.len() >= 2 {
        let min = set.iter().cloned().fold(f32::MAX, f32::min);
        let max = set.iter().cloned().fold(f32::MIN, f32::max);
        (min + max) / 2.0
    } else {
        90.0
    }
}

/// 트레이 아이콘 바로 아래에 팝오버 패널을 연다.
fn open_panel(
    target: &EventLoopWindowTarget<UserEvent>,
    tray: Option<&TrayIcon>,
    proxy: &EventLoopProxy<UserEvent>,
    html: &str,
) -> Result<(Window, wry::WebView), Box<dyn std::error::Error>> {
    let scale = target
        .primary_monitor()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);
    let pos = tray
        .and_then(|t| t.rect())
        .map(|r| {
            let p = r.position.to_logical::<f64>(scale);
            let s = r.size.to_logical::<f64>(scale);
            LogicalPosition::new(p.x + s.width / 2.0 - PANEL_W / 2.0, p.y + s.height + 6.0)
        })
        .unwrap_or(LogicalPosition::new(0.0, 34.0));

    let window = WindowBuilder::new()
        .with_title("MotionDesk")
        .with_inner_size(LogicalSize::new(PANEL_W, PANEL_H))
        .with_position(pos)
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top(true)
        .with_resizable(false)
        .build(target)?;
    let proxy = proxy.clone();
    let webview = wry::WebViewBuilder::new()
        .with_transparent(true)
        .with_ipc_handler(move |req| {
            let _ = proxy.send_event(UserEvent::Ipc(req.body().to_string()));
        })
        .with_html(html)
        .build(&window)?;
    window.set_focus(); // 포커스를 받아야 포커스 상실 시 자동 닫힘이 동작
    Ok((window, webview))
}

/// BLE 전담 스레드: 명령 채널을 소비하며 결과를 사용자 이벤트로 보고한다.
/// 이동(move_to)은 별도 태스크로 돌리므로, 이동 중에도 새 명령(다른 프리셋,
/// 정지)이 즉시 처리되어 진행 중인 이동을 중단시킨다.
fn ble_thread(mut cmd_rx: mpsc::UnboundedReceiver<DeskCmd>, proxy: EventLoopProxy<UserEvent>) {
    let rt = tokio::runtime::Runtime::new().expect("tokio 런타임 생성 실패");
    rt.block_on(async move {
        let title = |s: String| {
            let _ = proxy.send_event(UserEvent::Title(s));
        };
        let conn = |s: ConnState| {
            let _ = proxy.send_event(UserEvent::Conn(s));
        };

        let mut desk: Option<Arc<Desk>> = None;
        // 진행 중인 이동 태스크 — 새 이동/정지 명령이 오면 abort
        let mut mover: Option<tokio::task::JoinHandle<()>> = None;
        // 이동 태스크에서 통신 오류가 났을 때 연결을 버리라는 신호
        let (dead_tx, mut dead_rx) = mpsc::unbounded_channel::<()>();

        // 마지막으로 알려진 높이 — notify 태스크가 갱신, 하트비트 태스크가 기록
        let last_cm: Arc<Mutex<Option<f32>>> = Arc::new(Mutex::new(None));
        {
            // 5분마다 현재 높이를 이력에 기록 (움직임이 없어도 차트가 이어지도록)
            let last_cm = last_cm.clone();
            tokio::spawn(async move {
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

        loop {
            tokio::select! {
                maybe_cmd = cmd_rx.recv() => {
                    let Some(cmd) = maybe_cmd else { break };

                    // 연결이 없으면 먼저 연결 시도
                    if desk.is_none() {
                        title("↕ 연결 중...".into());
                        conn(ConnState::Connecting);
                        match Desk::connect(SCAN_TIMEOUT).await {
                            Ok(d) => {
                                // 연결 시점 높이를 이력에 기록
                                if let Ok(h) = d.height().await {
                                    history::append(h.cm);
                                    *last_cm.lock().unwrap() = Some(h.cm);
                                }
                                // 위치 notify 구독: 물리 스위치 조작도 실시간 반영
                                match d.subscribe_height().await {
                                    Ok(stream) => {
                                        let p = proxy.clone();
                                        let last_cm = last_cm.clone();
                                        tokio::spawn(async move {
                                            let mut stream = Box::pin(stream);
                                            let mut last_logged: Option<(i64, u16)> = None;
                                            while let Some(h) = stream.next().await {
                                                let _ = p.send_event(UserEvent::Title(
                                                    format!("↕ {:.0}cm", h.cm),
                                                ));
                                                *last_cm.lock().unwrap() = Some(h.cm);
                                                // 이동 중 이력: raw 변화 시 초당 1회
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
                                desk = Some(Arc::new(d));
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
                    let d = desk.clone().unwrap();

                    match cmd {
                        DeskCmd::GoTo(cm) => {
                            // 진행 중인 이동이 있으면 중단하고 새 목표로 교체
                            if let Some(m) = mover.take() {
                                m.abort();
                            }
                            title(format!("↕ {:.0}cm로 이동 중...", cm));
                            let p = proxy.clone();
                            let dead = dead_tx.clone();
                            mover = Some(tokio::spawn(async move {
                                match d.move_to(cm).await {
                                    Ok(h) => {
                                        let _ = p.send_event(UserEvent::Title(format!(
                                            "↕ {:.0}cm",
                                            h.cm
                                        )));
                                    }
                                    Err(e) => {
                                        eprintln!("이동 실패: {}", e);
                                        let _ = dead.send(());
                                    }
                                }
                            }));
                        }
                        DeskCmd::Stop => {
                            if let Some(m) = mover.take() {
                                m.abort();
                            }
                            match d.stop().await.and(d.height().await) {
                                Ok(h) => title(format!("↕ {:.0}cm", h.cm)),
                                Err(e) => {
                                    eprintln!("정지 실패: {}", e);
                                    let _ = dead_tx.send(());
                                }
                            }
                        }
                        DeskCmd::Refresh => match d.height().await {
                            Ok(h) => title(format!("↕ {:.0}cm", h.cm)),
                            Err(e) => {
                                eprintln!("높이 읽기 실패: {}", e);
                                let _ = dead_tx.send(());
                            }
                        },
                        DeskCmd::SaveSlot(slot) => match d.height().await {
                            Ok(h) => {
                                let _ = proxy.send_event(UserEvent::SlotSaved(slot, h.cm));
                                title(format!("↕ {:.0}cm", h.cm));
                            }
                            Err(e) => {
                                eprintln!("높이 읽기 실패: {}", e);
                                let _ = dead_tx.send(());
                            }
                        },
                    }
                }
                Some(_) = dead_rx.recv() => {
                    // 통신 오류 → 연결을 버리고 다음 명령에서 재연결
                    if let Some(m) = mover.take() {
                        m.abort();
                    }
                    desk = None;
                    title("↕ 연결 안 됨".into());
                    conn(ConnState::Disconnected);
                }
            }
        }
    });
}
