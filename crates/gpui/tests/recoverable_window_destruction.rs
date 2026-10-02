#![cfg(all(target_os = "windows", feature = "test-support"))]

#[path = "support/recoverable_native.rs"]
mod native;

use gpui::{
    AppContext as _, Empty, TestAppContext, WindowOptions,
    WindowsNativeConfirmationOutcome as ConfirmationOutcome,
    WindowsNativeConfirmationRequest as Request,
    WindowsNativeConfirmationTestFault as ConfirmationFault,
    WindowsNativeWindowDestructionOutcome as Outcome,
    WindowsNativeWindowDestructionTestFault as Fault,
    with_windows_window_destruction_fault_for_test,
    with_windows_window_destruction_observer_for_test,
};
use std::{cell::Cell, rc::Rc};

fn request() -> Request {
    Request::new(
        "GPUI recoverable owner",
        "Confirm action",
        "Cancel",
        "Confirm",
    )
    .unwrap()
}

#[test]
fn unsupported_backend_refuses_admission_without_removing_window() {
    let mut cx = TestAppContext::single();
    let handle = cx.update(|app| {
        app.open_window(WindowOptions::default(), |_, cx| cx.new(|_| Empty))
            .unwrap()
    });
    handle
        .update(&mut cx, |_, window, _| {
            assert!(window.begin_windows_native_destruction().is_err());
        })
        .unwrap();
    cx.update(|app| assert_eq!(app.windows().len(), 1));
}

#[test]
fn native_success_retains_slot_until_destruction_and_closes_once() {
    let attempts = Rc::new(Cell::new(0));
    let observed_attempts = attempts.clone();
    let closed = Rc::new(Cell::new(0));
    let observed_closed = closed.clone();
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    with_windows_window_destruction_observer_for_test(
        move |_| observed_attempts.set(observed_attempts.get() + 1),
        || {
            native::run(false, move |app| {
                let (handle, raw) = native::open(app, "success");
                app.on_window_closed(move |app| {
                    assert!(!native::alive(raw));
                    assert!(app.windows().is_empty());
                    observed_closed.set(observed_closed.get() + 1);
                })
                .detach();
                app.spawn(async move |cx| {
                    let (attempt, completion) = handle
                        .update(cx, |_, window, _| {
                            let admitted = window.begin_windows_native_destruction().unwrap();
                            assert!(window.begin_windows_native_destruction().is_err());
                            window.remove_window();
                            admitted
                        })
                        .unwrap();
                    assert!(attempt.outcome().is_none());
                    cx.update(|app| assert_eq!(app.windows().len(), 1)).unwrap();
                    assert!(native::alive(raw));
                    assert_eq!(completion.await.unwrap(), Outcome::Destroyed);
                    assert_eq!(attempt.outcome(), Some(Outcome::Destroyed));
                    attempt.replay_completion_for_test(Outcome::Destroyed);
                    cx.update(|app| assert!(app.windows().is_empty())).unwrap();
                    done.set(true);
                    cx.update(|app| app.quit()).unwrap();
                })
                .detach();
            });
        },
    );
    assert!(completed.get());
    assert_eq!(closed.get(), 1);
    assert_eq!(attempts.get(), 1);
}

#[test]
fn refusal_preserves_exact_root_and_window_then_fresh_attempt_succeeds() {
    let attempts = Rc::new(Cell::new(0));
    let observed = attempts.clone();
    let closed = Rc::new(Cell::new(0));
    let observed_closed = closed.clone();
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    with_windows_window_destruction_observer_for_test(
        move |_| observed.set(observed.get() + 1),
        || {
            native::run(false, move |app| {
                app.on_window_closed(move |_| observed_closed.set(observed_closed.get() + 1))
                    .detach();
                let (handle, raw) = native::open(app, "refusal");
                let root_id = handle
                    .update(app, |_, window, _| {
                        window.root::<Empty>().flatten().unwrap().entity_id()
                    })
                    .unwrap();
                app.spawn(async move |cx| {
                    let (old_attempt, completion) =
                        with_windows_window_destruction_fault_for_test(Fault::Refuse, || {
                            handle
                                .update(cx, |_, window, _| {
                                    window.begin_windows_native_destruction()
                                })
                                .unwrap()
                                .unwrap()
                        });
                    assert!(matches!(
                        completion.await.unwrap(),
                        Outcome::Survived { .. }
                    ));
                    assert!(native::alive(raw));
                    handle
                        .update(cx, |_, window, _| {
                            assert_eq!(
                                window.root::<Empty>().flatten().unwrap().entity_id(),
                                root_id
                            );
                            window.capture_windows_window_placement().unwrap();
                        })
                        .unwrap();
                    let old_outcome = old_attempt.outcome();
                    let (fresh, completion) = handle
                        .update(cx, |_, window, _| {
                            assert_eq!(
                                window.root::<Empty>().flatten().unwrap().entity_id(),
                                root_id
                            );
                            window.begin_windows_native_destruction().unwrap()
                        })
                        .unwrap();
                    assert!(!old_attempt.same_attempt(&fresh));
                    old_attempt.replay_completion_for_test(Outcome::Destroyed);
                    assert_eq!(old_attempt.outcome(), old_outcome);
                    assert!(fresh.outcome().is_none());
                    assert!(native::alive(raw));
                    assert_eq!(completion.await.unwrap(), Outcome::Destroyed);
                    assert_eq!(fresh.outcome(), Some(Outcome::Destroyed));
                    done.set(true);
                    cx.update(|app| app.quit()).unwrap();
                })
                .detach();
            });
        },
    );
    assert!(completed.get());
    assert_eq!(closed.get(), 1);
    assert_eq!(attempts.get(), 2);
}

#[test]
fn abandoned_receivers_and_controls_leave_native_cleanup_owned() {
    let attempts = Rc::new(Cell::new(0));
    let observed = attempts.clone();
    let closed = Rc::new(Cell::new(0));
    let observed_closed = closed.clone();
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    with_windows_window_destruction_observer_for_test(
        move |_| observed.set(observed.get() + 1),
        || {
            native::run(false, move |app| {
                app.on_window_closed(move |_| observed_closed.set(observed_closed.get() + 1))
                    .detach();
                let (handle, raw) = native::open(app, "abandon");
                app.spawn(async move |cx| {
                    let (attempt, completion) =
                        with_windows_window_destruction_fault_for_test(Fault::Refuse, || {
                            handle
                                .update(cx, |_, window, _| {
                                    window.begin_windows_native_destruction()
                                })
                                .unwrap()
                                .unwrap()
                        });
                    drop(completion);
                    while attempt.outcome().is_none() {
                        native::pump(cx).await;
                    }
                    assert!(matches!(attempt.outcome(), Some(Outcome::Survived { .. })));
                    assert!(native::alive(raw));
                    let (fresh, completion) = handle
                        .update(cx, |_, window, _| window.begin_windows_native_destruction())
                        .unwrap()
                        .unwrap();
                    drop(completion);
                    drop(fresh);
                    drop(attempt);
                    while native::alive(raw) {
                        native::pump(cx).await;
                    }
                    cx.update(|app| assert!(app.windows().is_empty())).unwrap();
                    done.set(true);
                    cx.update(|app| app.quit()).unwrap();
                })
                .detach();
            });
        },
    );
    assert!(completed.get());
    assert_eq!(closed.get(), 1);
    assert_eq!(attempts.get(), 2);
}

#[test]
fn destruction_cancels_queued_and_active_confirmation_before_owner_removal() {
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    native::run(false, move |app| {
        let (handle, _) = native::open(app, "queued-confirmation");
        app.spawn(async move |cx| {
            for active in [false, true] {
                let (handle, raw) = if active {
                    cx.update(|app| native::open(app, "active-confirmation"))
                        .unwrap()
                } else {
                    let raw = handle
                        .update(cx, |_, window, _| {
                            let (lease, _) = window.lease_published_windows_window().unwrap();
                            let raw = lease.raw_handle();
                            drop(lease);
                            raw
                        })
                        .unwrap();
                    native::pump(cx).await;
                    (handle, windows::Win32::Foundation::HWND(raw as *mut _))
                };
                let (confirmation, settled) = handle
                    .update(cx, |_, window, _| {
                        window.begin_windows_native_confirmation(request())
                    })
                    .unwrap()
                    .unwrap();
                let popup = if active {
                    Some(native::dialog(raw, cx).await)
                } else {
                    None
                };
                let (_, completion) = handle
                    .update(cx, |_, window, _| window.begin_windows_native_destruction())
                    .unwrap()
                    .unwrap();
                assert!(native::alive(raw));
                assert_eq!(settled.await.unwrap(), ConfirmationOutcome::Cancelled);
                assert!(confirmation.cleanup_settled());
                if let Some(popup) = popup {
                    assert!(!native::alive(popup));
                }
                assert_eq!(completion.await.unwrap(), Outcome::Destroyed);
                assert!(!native::alive(raw));
            }
            done.set(true);
            cx.update(|app| app.quit()).unwrap();
        })
        .detach();
    });
    assert!(completed.get());
}

#[test]
fn lost_native_settlement_retains_slot_and_excludes_removal_and_retry() {
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    let attempts = Rc::new(Cell::new(0));
    let observed = attempts.clone();
    let closed = Rc::new(Cell::new(0));
    let observed_closed = closed.clone();
    with_windows_window_destruction_observer_for_test(
        move |_| observed.set(observed.get() + 1),
        || {
            native::run(true, move |app| {
                app.on_window_closed(move |_| observed_closed.set(observed_closed.get() + 1))
                    .detach();
                let (handle, raw) = native::open(app, "missing-settlement");
                let root_id = handle
                    .update(app, |_, window, _| {
                        window.root::<Empty>().flatten().unwrap().entity_id()
                    })
                    .unwrap();
                app.spawn(async move |cx| {
                    let (attempt, completion) = with_windows_window_destruction_fault_for_test(
                        Fault::MissingSettlement,
                        || {
                            handle
                                .update(cx, |_, window, _| {
                                    window.begin_windows_native_destruction()
                                })
                                .unwrap()
                                .unwrap()
                        },
                    );
                    assert!(matches!(
                        completion.await.unwrap(),
                        Outcome::Unresolved { .. }
                    ));
                    assert!(!native::alive(raw));
                    handle
                        .update(cx, |_, window, _| {
                            assert_eq!(
                                window.root::<Empty>().flatten().unwrap().entity_id(),
                                root_id
                            );
                            assert!(window.begin_windows_native_destruction().is_err());
                            window.remove_window();
                            window.refresh();
                            window.set_window_title("retained destroyed wrapper");
                            window.set_background_appearance(
                                gpui::WindowBackgroundAppearance::Opaque,
                            );
                            window.resize(gpui::size(gpui::px(320.0), gpui::px(240.0)));
                            assert!(!window.is_maximized());
                            let _ = window.window_bounds();
                            let _ = window.mouse_position();
                        })
                        .unwrap();
                    native::pump(cx).await;
                    let (other, other_raw) = cx
                        .update(|app| native::open(app, "surviving-other"))
                        .unwrap();
                    attempt.refresh_native_windows_for_test();
                    other
                        .update(cx, |_, window, _| window.remove_window())
                        .unwrap();
                    while native::alive(other_raw) {
                        native::pump(cx).await;
                    }
                    native::pump(cx).await;
                    attempt.refresh_native_windows_for_test();
                    cx.update(|app| assert_eq!(app.windows().len(), 1)).unwrap();
                    assert!(matches!(
                        attempt.outcome(),
                        Some(Outcome::Unresolved { .. })
                    ));
                    done.set(true);
                    cx.update(|app| app.quit()).unwrap();
                })
                .detach();
            });
        },
    );
    assert!(completed.get());
    assert_eq!(closed.get(), 1);
    assert_eq!(attempts.get(), 2);
}

#[test]
fn lost_confirmation_cleanup_excludes_owner_destruction() {
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    native::run(false, move |app| {
        let (handle, raw) = native::open(app, "missing-confirmation-cleanup");
        app.spawn(async move |cx| {
            let (confirmation, settled) = handle
                .update(cx, |_, window, _| {
                    window.begin_windows_native_confirmation(
                        request().with_fault_for_test(ConfirmationFault::MissingDestruction),
                    )
                })
                .unwrap()
                .unwrap();
            let _ = native::dialog(raw, cx).await;
            let (_, completion) = handle
                .update(cx, |_, window, _| window.begin_windows_native_destruction())
                .unwrap()
                .unwrap();
            assert!(settled.await.is_err());
            assert!(!confirmation.cleanup_settled());
            assert!(matches!(
                completion.await.unwrap(),
                Outcome::Unresolved { .. }
            ));
            assert!(native::alive(raw));
            handle
                .update(cx, |_, window, _| {
                    assert!(window.begin_windows_native_destruction().is_err());
                    window.remove_window();
                })
                .unwrap();
            cx.update(|app| assert_eq!(app.windows().len(), 1)).unwrap();
            done.set(true);
            cx.update(|app| app.quit()).unwrap();
        })
        .detach();
    });
    assert!(completed.get());
}

#[test]
fn recoverable_success_preserves_default_last_window_exit() {
    let closed = Rc::new(Cell::new(0));
    let observed_closed = closed.clone();
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    native::run(true, move |app| {
        let (handle, raw) = native::open(app, "default-exit");
        app.on_window_closed(move |app| {
            assert!(!native::alive(raw));
            assert!(app.windows().is_empty());
            observed_closed.set(observed_closed.get() + 1);
        })
        .detach();
        let (_, completion) = handle
            .update(app, |_, window, _| {
                window.begin_windows_native_destruction()
            })
            .unwrap()
            .unwrap();
        app.spawn(async move |_| {
            assert_eq!(completion.await.unwrap(), Outcome::Destroyed);
            done.set(true);
        })
        .detach();
    });
    assert_eq!(closed.get(), 1);
    assert!(completed.get());
}
