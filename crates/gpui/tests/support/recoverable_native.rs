use gpui::{
    App, AppContext, Application, AsyncApp, Empty, Task, TitlebarOptions, WindowHandle,
    WindowOptions,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{
            FindWindowW, GW_ENABLEDPOPUP, GetWindow, IsWindow, IsWindowVisible,
        },
    },
    core::PCWSTR,
};

pub fn run(automatic_quit: bool, launch: impl FnOnce(&mut App) + 'static) {
    let timed_out = Rc::new(Cell::new(false));
    let timeout_flag = timed_out.clone();
    let watchdog = Rc::new(RefCell::new(None::<Task<()>>));
    let retained = watchdog.clone();
    Application::new()
        .with_quit_on_last_window_close(automatic_quit)
        .run(move |app| {
            *retained.borrow_mut() = Some(app.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_secs(10))
                    .await;
                timeout_flag.set(true);
                cx.update(|app| app.quit()).unwrap();
            }));
            launch(app);
        });
    drop(watchdog.borrow_mut().take());
    assert!(
        !timed_out.get(),
        "native recovery test exceeded its deadline"
    );
}

pub fn open(app: &mut App, name: &str) -> (WindowHandle<Empty>, HWND) {
    let title = format!("GPUI recoverable {} {name}", std::process::id());
    let wide = title.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let handle = app
        .open_window(
            WindowOptions {
                show: false,
                focus: false,
                titlebar: Some(TitlebarOptions {
                    title: Some(title.into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Empty),
        )
        .unwrap();
    let raw = unsafe { FindWindowW(None, PCWSTR(wide.as_ptr())) }.unwrap();
    handle
        .update(app, |_, window, app| window.publish(app))
        .unwrap()
        .unwrap();
    (handle, raw)
}

pub fn alive(raw: HWND) -> bool {
    unsafe { IsWindow(Some(raw)) }.as_bool()
}

pub async fn pump(cx: &AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(10))
        .await;
}

pub async fn dialog(raw: HWND, cx: &AsyncApp) -> HWND {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(popup) = unsafe { GetWindow(raw, GW_ENABLEDPOPUP) } {
            if popup != raw && unsafe { IsWindowVisible(popup) }.as_bool() {
                return popup;
            }
        }
        assert!(
            Instant::now() < deadline,
            "native confirmation did not appear"
        );
        pump(cx).await;
    }
}
