#![cfg(all(target_os = "windows", feature = "test-support"))]

#[path = "support/recoverable_native.rs"]
mod native;

use gpui::{
    App, AppContext as _, Empty, Pixels, Size, TitlebarOptions, WindowBounds, WindowHandle,
    WindowMinimumUpdateError as Error, WindowMinimumUpdateFailure as Failure,
    WindowMinimumUpdateOutcome as Outcome, WindowOptions, WindowsWindowMinimumTestFault as Fault,
    bounds, point, px, size, with_windows_window_minimum_fault_for_test,
};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, RECT, WPARAM},
        UI::{
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{GetActiveWindow, GetFocus},
            WindowsAndMessaging::*,
        },
    },
    core::PCWSTR,
};

fn open(
    app: &mut App,
    name: &str,
    state: fn(gpui::Bounds<Pixels>) -> WindowBounds,
    minimum: Option<Size<Pixels>>,
) -> (WindowHandle<Empty>, HWND) {
    open_with_frame(app, name, state, minimum, false)
}

fn open_with_frame(
    app: &mut App,
    name: &str,
    state: fn(gpui::Bounds<Pixels>) -> WindowBounds,
    minimum: Option<Size<Pixels>>,
    transparent: bool,
) -> (WindowHandle<Empty>, HWND) {
    let title = format!("GPUI minimum {} {name}", std::process::id());
    let wide = title.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let handle = app
        .open_window(
            WindowOptions {
                show: false,
                focus: false,
                window_bounds: Some(state(bounds(
                    point(px(70.), px(80.)),
                    size(px(220.), px(180.)),
                ))),
                window_min_size: minimum,
                titlebar: Some(TitlebarOptions {
                    title: Some(title.into()),
                    appears_transparent: transparent,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Empty),
        )
        .unwrap();
    let raw = unsafe { FindWindowW(None, PCWSTR(wide.as_ptr())) }.unwrap();
    (handle, raw)
}

fn client(raw: HWND) -> (i32, i32) {
    let mut rect = RECT::default();
    unsafe { GetClientRect(raw, &mut rect) }.unwrap();
    (rect.right - rect.left, rect.bottom - rect.top)
}

fn outer(raw: HWND) -> RECT {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(raw, &mut rect) }.unwrap();
    rect
}

fn placement(raw: HWND) -> WINDOWPLACEMENT {
    let mut placement = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    unsafe { GetWindowPlacement(raw, &mut placement) }.unwrap();
    placement
}

fn assert_fits(raw: HWND, minimum: Size<Pixels>) {
    let scale = f64::from(unsafe { GetDpiForWindow(raw) } as f32 / 96.);
    let actual = client(raw);
    assert!(f64::from(actual.0) >= (f64::from(minimum.width) * scale).ceil());
    assert!(f64::from(actual.1) >= (f64::from(minimum.height) * scale).ceil());
}

fn logical_extent(raw: HWND, physical: i32) -> Pixels {
    let scale = f64::from(unsafe { GetDpiForWindow(raw) } as f32 / 96.);
    let mut logical = (f64::from(physical) / scale) as f32;
    if (f64::from(logical) * scale).ceil() > f64::from(physical) {
        logical = logical.next_down();
    } else if (f64::from(logical) * scale).ceil() < f64::from(physical) {
        logical = logical.next_up();
    }
    assert_eq!((f64::from(logical) * scale).ceil(), f64::from(physical));
    px(logical)
}

#[test]
fn native_maximized_minimum_accepts_exact_normal_client_and_refuses_one_pixel_more() {
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    native::run(false, move |app| {
        app.spawn(async move |cx| {
            for transparent in [true, false] {
                let (handle, raw) = cx
                    .update(|app| {
                        open_with_frame(
                            app,
                            &format!("tight-{transparent}"),
                            WindowBounds::Windowed,
                            None,
                            transparent,
                        )
                    })
                    .unwrap();
                handle
                    .update(cx, |_, window, app| window.publish(app))
                    .unwrap()
                    .unwrap();
                let normal_client = client(raw);
                let normal_outer = outer(raw);
                let minimum = size(
                    logical_extent(raw, normal_client.0),
                    logical_extent(raw, normal_client.1),
                );
                let excessive = size(minimum.width, logical_extent(raw, normal_client.1 + 1));
                handle
                    .update(cx, |_, window, _| window.zoom_window())
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(3);
                while !unsafe { IsZoomed(raw) }.as_bool() {
                    assert!(Instant::now() < deadline);
                    native::pump(cx).await;
                }
                let before = placement(raw);
                let active = unsafe { GetActiveWindow() };
                let focused = unsafe { GetFocus() };
                assert_eq!(
                    before.rcNormalPosition.right - before.rcNormalPosition.left,
                    normal_outer.right - normal_outer.left
                );
                assert_eq!(
                    before.rcNormalPosition.bottom - before.rcNormalPosition.top,
                    normal_outer.bottom - normal_outer.top
                );
                assert_fits(raw, excessive);
                let completion = handle
                    .update(cx, |_, window, _| window.update_minimum_size(minimum))
                    .unwrap()
                    .unwrap();
                assert!(matches!(completion.await, Outcome::Applied { .. }));
                handle
                    .update(cx, |_, window, _| {
                        assert_eq!(window.minimum_size_for_test(), Some(minimum))
                    })
                    .unwrap();
                let completion = handle
                    .update(cx, |_, window, _| window.update_minimum_size(excessive))
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    completion.await,
                    Outcome::NotApplied(Failure::InsufficientSpace)
                );
                handle
                    .update(cx, |_, window, _| {
                        assert_eq!(window.minimum_size_for_test(), Some(minimum))
                    })
                    .unwrap();
                assert!(unsafe { IsZoomed(raw) }.as_bool());
                assert!(unsafe { IsWindowVisible(raw) }.as_bool());
                assert_eq!(placement(raw).rcNormalPosition, before.rcNormalPosition);
                assert_eq!(unsafe { GetActiveWindow() }, active);
                assert_eq!(unsafe { GetFocus() }, focused);
                handle
                    .update(cx, |_, window, _| window.remove_window())
                    .unwrap();
            }
            done.set(true);
            cx.update(|app| app.quit()).unwrap();
        })
        .detach();
    });
    assert!(completed.get());
}

#[test]
fn native_maximized_constructor_checks_exact_normal_client_without_exposure() {
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    native::run(false, move |app| {
        app.spawn(async move |cx| {
            for transparent in [true, false] {
                let (probe, raw) = cx
                    .update(|app| {
                        open_with_frame(
                            app,
                            &format!("restore-probe-{transparent}"),
                            WindowBounds::Maximized,
                            None,
                            transparent,
                        )
                    })
                    .unwrap();
                let normal = placement(raw).rcNormalPosition;
                let _ = unsafe { ShowWindow(raw, SW_RESTORE) };
                let deadline = Instant::now() + Duration::from_secs(3);
                while unsafe { IsZoomed(raw) }.as_bool() {
                    assert!(Instant::now() < deadline);
                    native::pump(cx).await;
                }
                let restored = outer(raw);
                assert_eq!(restored.right - restored.left, normal.right - normal.left);
                assert_eq!(restored.bottom - restored.top, normal.bottom - normal.top);
                let normal_client = client(raw);
                let minimum = size(
                    logical_extent(raw, normal_client.0),
                    logical_extent(raw, normal_client.1),
                );
                let excessive = size(minimum.width, logical_extent(raw, normal_client.1 + 1));
                probe
                    .update(cx, |_, window, _| window.remove_window())
                    .unwrap();
                let (handle, raw) = cx
                    .update(|app| {
                        open_with_frame(
                            app,
                            &format!("tight-construction-{transparent}"),
                            WindowBounds::Maximized,
                            Some(minimum),
                            transparent,
                        )
                    })
                    .unwrap();
                assert!(!unsafe { IsWindowVisible(raw) }.as_bool());
                assert_fits(raw, excessive);
                let before = placement(raw);
                assert_eq!(
                    before.rcNormalPosition.right - before.rcNormalPosition.left,
                    normal.right - normal.left
                );
                assert_eq!(
                    before.rcNormalPosition.bottom - before.rcNormalPosition.top,
                    normal.bottom - normal.top
                );
                let completion = handle
                    .update(cx, |_, window, _| window.update_minimum_size(excessive))
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    completion.await,
                    Outcome::NotApplied(Failure::InsufficientSpace)
                );
                assert_eq!(placement(raw).rcNormalPosition, before.rcNormalPosition);
                assert!(!unsafe { IsWindowVisible(raw) }.as_bool());
                handle
                    .update(cx, |_, window, _| {
                        assert_eq!(window.minimum_size_for_test(), Some(minimum))
                    })
                    .unwrap();
                handle
                    .update(cx, |_, window, _| window.remove_window())
                    .unwrap();
            }
            done.set(true);
            cx.update(|app| app.quit()).unwrap();
        })
        .detach();
    });
    assert!(completed.get());
}

#[test]
fn native_hidden_and_ordinary_minimum_updates_acknowledge_same_owner_and_tracking() {
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    native::run(false, move |app| {
        let (handle, raw) = open(app, "hidden", WindowBounds::Windowed, None);
        let original = outer(raw);
        let focus = unsafe { GetFocus() };
        let active = unsafe { GetActiveWindow() };
        app.spawn(async move |cx| {
            let minimum = size(px(350.25), px(120.));
            let root = handle.update(cx, |_, window, _| window.root::<Empty>().flatten().unwrap().entity_id()).unwrap();
            let completion = handle.update(cx, |_, window, _| {
                let completion = window.update_minimum_size(minimum).unwrap();
                assert_eq!(window.update_minimum_size(minimum).err(), Some(Error::Busy));
                completion
            }).unwrap();
            assert!(matches!(completion.await, Outcome::Applied { minimum: applied, .. } if applied == minimum));
            assert!(native::alive(raw));
            assert_fits(raw, minimum);
            assert!(!unsafe { IsWindowVisible(raw) }.as_bool());
            assert_eq!((outer(raw).left, outer(raw).top), (original.left, original.top));
            assert_eq!(outer(raw).bottom - outer(raw).top, original.bottom - original.top);
            assert_eq!(unsafe { GetFocus() }, focus);
            assert_eq!(unsafe { GetActiveWindow() }, active);
            let mut info = MINMAXINFO::default();
            unsafe { SendMessageW(raw, WM_GETMINMAXINFO, Some(WPARAM(0)), Some(LPARAM((&mut info as *mut MINMAXINFO) as isize))) };
            let scale = f64::from(unsafe { GetDpiForWindow(raw) } as f32 / 96.);
            let border = (outer(raw).right - outer(raw).left) - client(raw).0;
            assert_eq!(info.ptMinTrackSize.x, (f64::from(minimum.width) * scale).ceil() as i32 + border);
            let (lease, released) = handle.update(cx, |_, window, _| window.lease_hidden_windows_window()).unwrap().unwrap();
            assert_eq!(lease.raw_handle(), raw.0 as usize);
            drop(lease);
            assert!(!released.await.unwrap().native_destroyed);
            handle.update(cx, |_, window, app| window.publish(app)).unwrap().unwrap();
            assert!(unsafe { IsWindowVisible(raw) }.as_bool());
            let adequate = outer(raw);
            let completion = handle.update(cx, |_, window, _| window.update_minimum_size(size(px(50.), px(50.)))).unwrap().unwrap();
            assert!(matches!(completion.await, Outcome::Applied { .. }));
            let after = outer(raw);
            assert_eq!((after.left, after.top, after.right, after.bottom), (adequate.left, adequate.top, adequate.right, adequate.bottom));
            handle.update(cx, |_, window, _| { assert_eq!(window.root::<Empty>().flatten().unwrap().entity_id(), root); window.remove_window(); }).unwrap();
            done.set(true);
            cx.update(|app| app.quit()).unwrap();
        }).detach();
    });
    assert!(completed.get());
}

#[test]
fn native_minimized_maximized_and_fullscreen_preserve_state_and_restore_geometry() {
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    native::run(false, move |app| {
        let (handle, raw) = open(app, "state", WindowBounds::Windowed, None);
        handle
            .update(app, |_, window, app| window.publish(app))
            .unwrap()
            .unwrap();
        app.spawn(async move |cx| {
            handle
                .update(cx, |_, window, _| window.minimize_window())
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            while !unsafe { IsIconic(raw) }.as_bool() {
                assert!(Instant::now() < deadline);
                native::pump(cx).await;
            }
            handle
                .update(cx, |_, window, _| {
                    assert_eq!(window.is_minimized(), Some(true))
                })
                .unwrap();
            let before = placement(raw);
            let completion = handle
                .update(cx, |_, window, _| {
                    window.update_minimum_size(size(px(400.), px(300.)))
                })
                .unwrap()
                .unwrap();
            assert_eq!(completion.await, Outcome::NotApplied(Failure::Minimized));
            assert!(unsafe { IsIconic(raw) }.as_bool());
            assert_eq!(placement(raw).rcNormalPosition, before.rcNormalPosition);
            let _ = unsafe { ShowWindow(raw, SW_RESTORE) };
            let deadline = Instant::now() + Duration::from_secs(3);
            while unsafe { IsIconic(raw) }.as_bool() {
                assert!(Instant::now() < deadline);
                native::pump(cx).await;
            }
            handle
                .update(cx, |_, window, _| {
                    assert_eq!(window.is_minimized(), Some(false))
                })
                .unwrap();
            let completion = handle
                .update(cx, |_, window, _| {
                    window.update_minimum_size(size(px(260.), px(190.)))
                })
                .unwrap()
                .unwrap();
            assert!(matches!(completion.await, Outcome::Applied { .. }));
            handle
                .update(cx, |_, window, _| window.zoom_window())
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            while !unsafe { IsZoomed(raw) }.as_bool() {
                assert!(Instant::now() < deadline);
                native::pump(cx).await;
            }
            let before = placement(raw);
            let completion = handle
                .update(cx, |_, window, _| {
                    window.update_minimum_size(size(px(20.), px(20.)))
                })
                .unwrap()
                .unwrap();
            assert!(matches!(completion.await, Outcome::Applied { .. }));
            let completion = handle
                .update(cx, |_, window, _| {
                    window.update_minimum_size(size(px(1_000_000.), px(1_000_000.)))
                })
                .unwrap()
                .unwrap();
            assert_eq!(
                completion.await,
                Outcome::NotApplied(Failure::InsufficientSpace)
            );
            assert!(unsafe { IsZoomed(raw) }.as_bool());
            assert_eq!(placement(raw).rcNormalPosition, before.rcNormalPosition);
            handle
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
            let (fullscreen, fullscreen_raw) = cx
                .update(|app| open(app, "fullscreen", WindowBounds::Fullscreen, None))
                .unwrap();
            let before = outer(fullscreen_raw);
            let completion = fullscreen
                .update(cx, |_, window, _| {
                    window.update_minimum_size(size(px(20.), px(20.)))
                })
                .unwrap()
                .unwrap();
            assert_eq!(
                completion.await,
                Outcome::NotApplied(Failure::UnsupportedState)
            );
            assert_eq!(outer(fullscreen_raw), before);
            assert!(!unsafe { IsWindowVisible(fullscreen_raw) }.as_bool());
            fullscreen
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
            done.set(true);
            cx.update(|app| app.quit()).unwrap();
        })
        .detach();
    });
    assert!(completed.get());
}

#[test]
fn native_refusal_unverified_and_receiver_drop_preserve_constraint_and_settle_close() {
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    native::run(false, move |app| {
        let initial = size(px(100.), px(90.));
        let (handle, raw) = open(app, "failure", WindowBounds::Windowed, Some(initial));
        app.spawn(async move |cx| {
            for (fault, expected) in [
                (Fault::Refuse, Outcome::NotApplied(Failure::NativeFailure)),
                (Fault::Unverified, Outcome::Unverified),
            ] {
                let completion = handle
                    .update(cx, |_, window, _| {
                        with_windows_window_minimum_fault_for_test(fault, || {
                            window.update_minimum_size(size(px(500.), px(300.)))
                        })
                    })
                    .unwrap()
                    .unwrap();
                assert_eq!(completion.await, expected);
                handle
                    .update(cx, |_, window, _| {
                        assert_eq!(window.minimum_size_for_test(), Some(initial))
                    })
                    .unwrap();
                assert!(native::alive(raw));
                assert!(!unsafe { IsWindowVisible(raw) }.as_bool());
            }
            assert_fits(raw, size(px(500.), px(300.)));
            handle
                .update(cx, |_, window, _| {
                    assert_eq!(
                        window
                            .update_minimum_size(size(px(f32::NAN), px(30.)))
                            .err(),
                        Some(Error::InvalidMinimum)
                    );
                    drop(
                        window
                            .update_minimum_size(size(px(550.), px(330.)))
                            .unwrap(),
                    );
                })
                .unwrap();
            native::pump(cx).await;
            assert_fits(raw, size(px(550.), px(330.)));
            let completion = handle
                .update(cx, |_, window, _| {
                    let completion = window
                        .update_minimum_size(size(px(600.), px(350.)))
                        .unwrap();
                    window.remove_window();
                    completion
                })
                .unwrap();
            assert_eq!(completion.await, Outcome::NotApplied(Failure::Closed));
            assert!(!native::alive(raw));
            let (dropped, dropped_raw) = cx
                .update(|app| open(app, "droppedclose", WindowBounds::Windowed, None))
                .unwrap();
            dropped
                .update(cx, |_, window, _| {
                    drop(
                        window
                            .update_minimum_size(size(px(400.), px(250.)))
                            .unwrap(),
                    );
                    window.remove_window();
                })
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            while native::alive(dropped_raw) {
                assert!(Instant::now() < deadline);
                native::pump(cx).await;
            }
            done.set(true);
            cx.update(|app| app.quit()).unwrap();
        })
        .detach();
    });
    assert!(completed.get());
}

#[test]
fn native_constructor_applies_declared_minimum_before_first_exposure() {
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    native::run(false, move |app| {
        let minimum = size(px(360.), px(100.));
        let (handle, raw) = open(app, "construction", WindowBounds::Windowed, Some(minimum));
        assert_fits(raw, minimum);
        assert!(!unsafe { IsWindowVisible(raw) }.as_bool());
        let (maximized, maximized_raw) =
            open(app, "initialmaximized", WindowBounds::Maximized, None);
        app.spawn(async move |cx| {
            let completion = maximized
                .update(cx, |_, window, _| {
                    window.update_minimum_size(size(px(20.), px(20.)))
                })
                .unwrap()
                .unwrap();
            assert!(matches!(completion.await, Outcome::Applied { .. }));
            assert!(!unsafe { IsWindowVisible(maximized_raw) }.as_bool());
            maximized
                .update(cx, |_, window, app| window.publish(app))
                .unwrap()
                .unwrap();
            assert!(unsafe { IsZoomed(maximized_raw) }.as_bool());
            handle
                .update(cx, |_, window, app| window.publish(app))
                .unwrap()
                .unwrap();
            assert_fits(raw, minimum);
            handle
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
            maximized
                .update(cx, |_, window, _| window.remove_window())
                .unwrap();
            done.set(true);
            cx.update(|app| app.quit()).unwrap();
        })
        .detach();
    });
    assert!(completed.get());
}

#[test]
fn native_constructor_refuses_insufficient_maximized_restore_without_exposure() {
    let observed = Rc::new(Cell::new(0_usize));
    let capture = observed.clone();
    native::run(false, move |app| {
        let result = gpui::with_windows_window_creation_hook_for_test(
            move |raw| {
                capture.set(raw);
                Ok(())
            },
            || {
                app.open_window(
                    WindowOptions {
                        show: false,
                        focus: false,
                        window_bounds: Some(WindowBounds::Maximized(bounds(
                            point(px(70.), px(80.)),
                            size(px(220.), px(180.)),
                        ))),
                        window_min_size: Some(size(px(500.), px(300.))),
                        ..Default::default()
                    },
                    |_, cx| cx.new(|_| Empty),
                )
            },
        );
        assert!(result.is_err());
        assert!(app.windows().is_empty());
        app.quit();
    });
    assert_ne!(observed.get(), 0);
    assert!(!native::alive(HWND(observed.get() as *mut _)));
}
