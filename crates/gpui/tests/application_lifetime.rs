#![cfg(target_os = "windows")]

use gpui::{
    App, AppContext, Application, Empty, Task, TitlebarOptions, WindowHandle, WindowOptions,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
use windows::{
    Win32::UI::WindowsAndMessaging::{FindWindowW, IsWindow},
    core::PCWSTR,
};

fn run(application: Application, launch: impl FnOnce(&mut App) + 'static) {
    let timeout_task = Rc::new(RefCell::new(None::<Task<()>>));
    let retained_task = timeout_task.clone();
    let timed_out = Rc::new(Cell::new(false));
    let timeout_flag = timed_out.clone();
    let quits = Rc::new(Cell::new(0));
    let quit_count = quits.clone();
    application.run(move |app| {
        app.on_app_quit(move |_| {
            quit_count.set(quit_count.get() + 1);
            async {}
        })
        .detach();
        *retained_task.borrow_mut() = Some(app.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_secs(10))
                .await;
            timeout_flag.set(true);
            cx.update(|app| app.quit()).unwrap();
        }));
        launch(app);
    });
    drop(timeout_task.borrow_mut().take());
    assert!(
        !timed_out.get(),
        "application did not quit through the expected path"
    );
    assert_eq!(quits.get(), 1);
}

fn open(app: &mut App, name: &str) -> (WindowHandle<Empty>, windows::Win32::Foundation::HWND) {
    let title = format!("GPUI lifetime {} {name}", std::process::id());
    let wide = title.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let window = app
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
    (window, raw)
}

fn automatic_quit(application: Application) {
    let closed = Rc::new(Cell::new(false));
    let observed = closed.clone();
    run(application, move |app| {
        app.on_window_closed(move |_| observed.set(true)).detach();
        let (window, _) = open(app, "automatic");
        window
            .update(app, |_, window, _| window.remove_window())
            .unwrap();
    });
    assert!(closed.get());
}

#[test]
fn default_quits_after_last_window_destruction() {
    automatic_quit(Application::new());
}

#[test]
fn final_builder_true_restores_default() {
    automatic_quit(
        Application::new()
            .with_quit_on_last_window_close(false)
            .with_quit_on_last_window_close(true),
    );
}

#[test]
fn zero_window_continuation_reopens_and_explicitly_quits() {
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    let closed = Rc::new(Cell::new(0));
    let observed = closed.clone();
    run(
        Application::new()
            .with_quit_on_last_window_close(true)
            .with_quit_on_last_window_close(false),
        move |app| {
            app.on_window_closed(move |_| observed.set(observed.get() + 1))
                .detach();
            app.spawn(async move |cx| {
                for name in ["first", "successor"] {
                    let (window, raw) = cx.update(|app| open(app, name)).unwrap();
                    let destruction = window
                        .update(cx, |_, window, app| {
                            window.publish(app).unwrap();
                            window.observe_windows_native_destruction().unwrap()
                        })
                        .unwrap();
                    window
                        .update(cx, |_, window, _| window.remove_window())
                        .unwrap();
                    destruction.await.unwrap();
                    assert!(!unsafe { IsWindow(Some(raw)) }.as_bool());
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    cx.update(|app| assert!(app.windows().is_empty())).unwrap();
                }
                done.set(true);
                cx.update(|app| app.quit()).unwrap();
            })
            .detach();
        },
    );
    assert!(completed.get());
    assert_eq!(closed.get(), 2);
}

#[test]
fn explicit_quit_with_remaining_window_is_unchanged() {
    let requested = Rc::new(Cell::new(false));
    let observed = requested.clone();
    run(
        Application::new().with_quit_on_last_window_close(false),
        move |app| {
            let _ = open(app, "explicit");
            assert_eq!(app.windows().len(), 1);
            observed.set(true);
            app.quit();
        },
    );
    assert!(requested.get());
}
