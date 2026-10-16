#![cfg(feature = "test-support")]

use gpui::{
    AppContext as _, Context, IntoElement, Render, Styled, TestAppContext, Window, div, px, rgb,
};
use std::{cell::Cell, rc::Rc};

struct RenderWitness {
    revision: usize,
    draws: Rc<Cell<usize>>,
    painted_revision: Rc<Cell<usize>>,
}

impl Render for RenderWitness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        self.painted_revision.set(self.revision);
        div().w(px(80.)).h(px(40.)).bg(rgb(match self.revision {
            0 => 0x102030,
            1 => 0x405060,
            _ => 0x708090,
        }))
    }
}

fn assert_dirty_update_renders(cx: &mut TestAppContext) {
    let draws = Rc::new(Cell::new(0));
    let painted_revision = Rc::new(Cell::new(usize::MAX));
    let render_count = draws.clone();
    let rendered_value = painted_revision.clone();
    let window = cx.add_window(move |_, _| RenderWitness {
        revision: 0,
        draws: render_count,
        painted_revision: rendered_value,
    });
    assert!(draws.get() > 0);
    assert_eq!(painted_revision.get(), 0);
    let before = draws.get();

    cx.update(|app| {
        window
            .update(app, |root, _, cx| {
                root.revision = 1;
                cx.notify();
            })
            .unwrap();
    });

    assert!(draws.get() > before);
    assert_eq!(painted_revision.get(), 1);
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
}

#[test]
fn deterministic_single_renders_dirty_views_at_update_boundary() {
    let mut cx = TestAppContext::single();
    assert_dirty_update_renders(&mut cx);
    cx.quit();
}

#[test]
fn deterministic_new_app_renders_dirty_views_at_update_boundary() {
    let parent = TestAppContext::single();
    let mut child = parent.new_app();
    assert_dirty_update_renders(&mut child);
    child.quit();
    parent.quit();
}

#[test]
fn deterministic_clone_preserves_update_rendering_owner() {
    let mut owner = TestAppContext::single();
    let mut cloned = owner.clone();
    assert_dirty_update_renders(&mut cloned);
    assert_dirty_update_renders(&mut owner);
    owner.quit();
}

#[cfg(target_os = "windows")]
#[test]
fn native_hidden_dirty_views_render_before_same_window_publication() {
    use gpui::{Application, Task, WindowOptions};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::{cell::RefCell, time::Duration};
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::IsWindowVisible};

    let draws = Rc::new(Cell::new(0));
    let painted_revision = Rc::new(Cell::new(usize::MAX));
    let stage = Rc::new(Cell::new(0));
    let observed_stage = stage.clone();
    let render_count = draws.clone();
    let rendered_value = painted_revision.clone();
    let timed_out = Rc::new(Cell::new(false));
    let timeout_flag = timed_out.clone();
    let watchdog = Rc::new(RefCell::new(None::<Task<()>>));
    let retained_watchdog = watchdog.clone();

    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            *retained_watchdog.borrow_mut() = Some(app.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_secs(30))
                    .await;
                timeout_flag.set(true);
                cx.update(|app| app.quit()).unwrap();
            }));
            let view_draws = render_count.clone();
            let view_revision = rendered_value.clone();
            let handle = app
                .open_window(
                    WindowOptions {
                        show: false,
                        focus: false,
                        ..Default::default()
                    },
                    move |_, cx| {
                        cx.new(|_| RenderWitness {
                            revision: 0,
                            draws: view_draws,
                            painted_revision: view_revision,
                        })
                    },
                )
                .unwrap();
            assert!(render_count.get() > 0);
            assert_eq!(rendered_value.get(), 0);
            let before = render_count.get();
            let callback_draws = render_count.clone();
            let callback_revision = rendered_value.clone();

            handle
                .update(app, move |root, window, cx| {
                    let raw = window.window_handle().unwrap().as_raw();
                    let RawWindowHandle::Win32(win32) = raw else {
                        panic!("expected the original Windows native window");
                    };
                    let hwnd = HWND(win32.hwnd.get() as *mut _);
                    assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());
                    root.revision = 1;
                    cx.notify();
                    let root = cx.entity();
                    window.on_next_frame(move |window, _| {
                        assert_eq!(callback_draws.get(), before);
                        assert_eq!(callback_revision.get(), 0);
                        assert_eq!(window.window_handle().unwrap().as_raw(), raw);
                        assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());
                        observed_stage.set(1);
                        window.on_next_frame(move |window, app| {
                            assert!(callback_draws.get() > before);
                            assert_eq!(callback_revision.get(), 1);
                            assert_eq!(window.window_handle().unwrap().as_raw(), raw);
                            assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());
                            observed_stage.set(2);
                            let hidden_draws = callback_draws.get();
                            window.publish(app).unwrap();
                            root.update(app, |view, cx| {
                                view.revision = 2;
                                cx.notify();
                            });
                            window.on_next_frame(move |window, app| {
                                assert!(callback_draws.get() > hidden_draws);
                                assert_eq!(callback_revision.get(), 2);
                                assert_eq!(window.window_handle().unwrap().as_raw(), raw);
                                assert!(unsafe { IsWindowVisible(hwnd) }.as_bool());
                                observed_stage.set(3);
                                window.remove_window();
                                app.quit();
                            });
                        });
                    });
                })
                .unwrap();
            assert_eq!(render_count.get(), before);
            assert_eq!(rendered_value.get(), 0);
        });

    drop(watchdog.borrow_mut().take());
    assert!(
        !timed_out.get(),
        "native rendering deadline: stage={} draws={} revision={}",
        stage.get(),
        draws.get(),
        painted_revision.get()
    );
    assert_eq!(stage.get(), 3);
}

#[cfg(target_os = "windows")]
#[test]
fn native_hidden_frame_callback_can_close_without_publication() {
    use gpui::{Application, Empty, Task, WindowOptions};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::{cell::RefCell, time::Duration};
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{IsWindow, IsWindowVisible},
    };

    let callbacks = Rc::new(Cell::new(0));
    let callback_count = callbacks.clone();
    let hidden_observations = Rc::new(Cell::new(0));
    let observed_hidden = hidden_observations.clone();
    let original_hwnd = Rc::new(Cell::new(None::<HWND>));
    let captured_hwnd = original_hwnd.clone();
    let timed_out = Rc::new(Cell::new(false));
    let timeout_flag = timed_out.clone();
    let watchdog = Rc::new(RefCell::new(None::<Task<()>>));
    let retained_watchdog = watchdog.clone();

    Application::new().run(move |app| {
        *retained_watchdog.borrow_mut() = Some(app.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_secs(30))
                .await;
            timeout_flag.set(true);
            cx.update(|app| app.quit()).unwrap();
        }));
        let handle = app
            .open_window(
                WindowOptions {
                    show: false,
                    focus: false,
                    ..Default::default()
                },
                |_, cx| cx.new(|_| Empty),
            )
            .unwrap();
        handle
            .update(app, move |_, window, _| {
                let raw = window.window_handle().unwrap().as_raw();
                let RawWindowHandle::Win32(win32) = raw else {
                    panic!("expected the original Windows native window");
                };
                let hwnd = HWND(win32.hwnd.get() as *mut _);
                captured_hwnd.set(Some(hwnd));
                assert!(unsafe { IsWindow(Some(hwnd)) }.as_bool());
                assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());
                observed_hidden.set(observed_hidden.get() + 1);
                window.on_next_frame(move |window, _| {
                    assert_eq!(window.window_handle().unwrap().as_raw(), raw);
                    assert!(unsafe { IsWindow(Some(hwnd)) }.as_bool());
                    assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());
                    observed_hidden.set(observed_hidden.get() + 1);
                    callback_count.set(callback_count.get() + 1);
                    window.remove_window();
                });
            })
            .unwrap();
    });

    drop(watchdog.borrow_mut().take());
    assert!(
        !timed_out.get(),
        "hidden native closure deadline: callbacks={} hidden_observations={}",
        callbacks.get(),
        hidden_observations.get()
    );
    assert_eq!(callbacks.get(), 1);
    assert_eq!(hidden_observations.get(), 2);
    let hwnd = original_hwnd
        .get()
        .expect("original native window captured");
    assert!(!unsafe { IsWindow(Some(hwnd)) }.as_bool());
}
