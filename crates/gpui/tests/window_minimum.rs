#![cfg(feature = "test-support")]

use gpui::{
    AppContext as _, Bounds, Empty, Pixels, Size, TestAppContext, WindowBounds, WindowHandle,
    WindowMinimumUpdateError as Error, WindowMinimumUpdateFailure as Failure,
    WindowMinimumUpdateOutcome as Outcome, WindowOptions, point, px, size,
};

fn open(
    cx: &mut TestAppContext,
    bounds: WindowBounds,
    minimum: Option<Size<Pixels>>,
) -> WindowHandle<Empty> {
    cx.update(|app| {
        app.open_window(
            WindowOptions {
                show: false,
                focus: false,
                window_bounds: Some(bounds),
                window_min_size: minimum,
                ..Default::default()
            },
            |_, cx| cx.new(|_| Empty),
        )
    })
    .unwrap()
}

fn bounds() -> Bounds<Pixels> {
    Bounds {
        origin: point(px(17.), px(29.)),
        size: size(px(90.), px(80.)),
    }
}

fn apply(cx: &mut TestAppContext, window: WindowHandle<Empty>, minimum: Size<Pixels>) -> Outcome {
    let completion = window
        .update(cx, |_, window, _| window.update_minimum_size(minimum))
        .unwrap()
        .unwrap();
    cx.executor().block_test(completion)
}

#[test]
fn hidden_growth_preserves_origin_identity_and_publication_and_retains_constraint() {
    let mut cx = TestAppContext::single();
    let window = open(&mut cx, WindowBounds::Windowed(bounds()), None);
    let root = window
        .update(&mut cx, |_, window, _| {
            window.root::<Empty>().flatten().unwrap().entity_id()
        })
        .unwrap();
    let minimum = size(px(120.25), px(70.));
    assert_eq!(
        apply(&mut cx, window, minimum),
        Outcome::Applied {
            minimum,
            client_size: size(px(120.5), px(80.))
        }
    );
    assert!(!cx.window_visibility(window.into()).is_visible);
    window
        .update(&mut cx, |_, window, _| {
            assert_eq!(window.bounds().origin, bounds().origin);
            assert_eq!(window.root::<Empty>().flatten().unwrap().entity_id(), root);
            assert_eq!(window.minimum_size_for_test(), Some(minimum));
        })
        .unwrap();
    let adequate = size(px(50.), px(50.));
    assert_eq!(
        apply(&mut cx, window, adequate),
        Outcome::Applied {
            minimum: adequate,
            client_size: size(px(120.5), px(80.))
        }
    );
    window
        .update(&mut cx, |_, window, app| window.publish(app))
        .unwrap()
        .unwrap();
    assert!(cx.window_visibility(window.into()).is_visible);
    assert_eq!(
        cx.window_visibility(window.into()).activation_change_count,
        0
    );
}

#[test]
fn invalid_busy_and_receiver_drop_do_not_abandon_original_operation() {
    let mut cx = TestAppContext::single();
    let window = open(&mut cx, WindowBounds::Windowed(bounds()), None);
    window
        .update(&mut cx, |_, window, _| {
            for value in [0., -1., f32::NAN, f32::INFINITY, f32::MAX] {
                assert_eq!(
                    window.update_minimum_size(size(px(value), px(50.))).err(),
                    Some(Error::InvalidMinimum)
                );
            }
            let completion = window
                .update_minimum_size(size(px(120.), px(100.)))
                .unwrap();
            assert_eq!(
                window.update_minimum_size(size(px(130.), px(100.))).err(),
                Some(Error::Busy)
            );
            drop(completion);
        })
        .unwrap();
    cx.run_until_parked();
    window
        .update(&mut cx, |_, window, _| {
            assert_eq!(
                window.minimum_size_for_test(),
                Some(size(px(120.), px(100.)))
            )
        })
        .unwrap();
    assert!(matches!(
        apply(&mut cx, window, size(px(130.), px(110.))),
        Outcome::Applied { .. }
    ));
}

#[test]
fn nonapplied_states_and_failures_preserve_previous_constraint() {
    for (bounds, expected) in [
        (
            WindowBounds::Maximized(bounds()),
            Failure::InsufficientSpace,
        ),
        (
            WindowBounds::Fullscreen(bounds()),
            Failure::UnsupportedState,
        ),
    ] {
        let mut cx = TestAppContext::single();
        let window = open(&mut cx, bounds, None);
        assert_eq!(
            apply(&mut cx, window, size(px(200.), px(100.))),
            Outcome::NotApplied(expected)
        );
        window
            .update(&mut cx, |_, window, _| {
                assert_eq!(window.minimum_size_for_test(), None)
            })
            .unwrap();
    }
    let mut cx = TestAppContext::single();
    let window = open(&mut cx, WindowBounds::Maximized(bounds()), None);
    cx.simulate_window_resize(window.into(), size(px(400.), px(300.)));
    assert_eq!(
        apply(&mut cx, window, size(px(200.), px(100.))),
        Outcome::NotApplied(Failure::InsufficientSpace)
    );
    assert!(matches!(
        apply(&mut cx, window, size(px(50.), px(50.))),
        Outcome::Applied { .. }
    ));
    let window = open(
        &mut cx,
        WindowBounds::Windowed(bounds()),
        Some(size(px(60.), px(50.))),
    );
    window
        .update(&mut cx, |_, window, app| {
            window.publish(app).unwrap();
            assert_eq!(window.is_minimized(), Some(false));
            window.minimize_window();
            assert_eq!(window.is_minimized(), Some(true));
        })
        .unwrap();
    assert_eq!(
        apply(&mut cx, window, size(px(100.), px(100.))),
        Outcome::NotApplied(Failure::Minimized)
    );
    window
        .update(&mut cx, |_, window, _| {
            assert_eq!(window.minimum_size_for_test(), Some(size(px(60.), px(50.))));
            window.activate_window();
            assert_eq!(window.is_minimized(), Some(false));
        })
        .unwrap();
    assert!(matches!(
        apply(&mut cx, window, size(px(100.), px(100.))),
        Outcome::Applied { .. }
    ));
    for unverified in [false, true] {
        let window = open(
            &mut cx,
            WindowBounds::Windowed(bounds()),
            Some(size(px(60.), px(50.))),
        );
        window
            .update(&mut cx, |_, window, _| {
                window.fail_next_minimum_update_for_test(unverified)
            })
            .unwrap();
        assert_eq!(
            apply(&mut cx, window, size(px(100.), px(100.))),
            if unverified {
                Outcome::Unverified
            } else {
                Outcome::NotApplied(Failure::NativeFailure)
            }
        );
        window
            .update(&mut cx, |_, window, _| {
                assert_eq!(window.minimum_size_for_test(), Some(size(px(60.), px(50.))))
            })
            .unwrap();
    }
}

#[test]
fn constructor_clamps_declared_dimensions_and_removal_settles_admitted_operation() {
    let mut cx = TestAppContext::single();
    let window = open(
        &mut cx,
        WindowBounds::Windowed(bounds()),
        Some(size(px(110.), px(0.))),
    );
    window
        .update(&mut cx, |_, window, _| {
            assert_eq!(window.bounds().origin, bounds().origin);
            assert_eq!(window.viewport_size(), size(px(110.), px(80.)));
        })
        .unwrap();
    let completion = window
        .update(&mut cx, |_, window, _| {
            let completion = window
                .update_minimum_size(size(px(120.), px(100.)))
                .unwrap();
            window.remove_window();
            completion
        })
        .unwrap();
    assert_eq!(
        cx.executor().block_test(completion),
        Outcome::NotApplied(Failure::Closed)
    );
    assert!(window.update(&mut cx, |_, _, _| ()).is_err());
    for bounds in [
        WindowBounds::Maximized(bounds()),
        WindowBounds::Fullscreen(bounds()),
    ] {
        let failed = cx.update(|app| {
            app.open_window(
                WindowOptions {
                    show: false,
                    window_bounds: Some(bounds),
                    window_min_size: Some(size(px(200.), px(100.))),
                    ..Default::default()
                },
                |_, cx| cx.new(|_| Empty),
            )
        });
        assert!(failed.is_err());
    }
}
