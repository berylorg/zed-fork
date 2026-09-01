#[cfg(feature = "test-support")]
use gpui::{AnyWindowHandle, AppContext as _, Empty, TestAppContext, WindowBounds, WindowOptions};

#[cfg(not(feature = "test-support"))]
#[test]
fn window_publication_requires_test_support() {}

#[cfg(feature = "test-support")]
fn open_window_with_bounds(
    cx: &mut TestAppContext,
    show: bool,
    focus: bool,
    window_bounds: Option<WindowBounds>,
) -> AnyWindowHandle {
    cx.update(|cx| {
        cx.open_window(
            WindowOptions {
                show,
                focus,
                window_bounds,
                ..Default::default()
            },
            |_, cx| cx.new(|_| Empty),
        )
    })
    .unwrap()
    .into()
}

#[cfg(feature = "test-support")]
fn open_window(cx: &mut TestAppContext, show: bool) -> AnyWindowHandle {
    open_window_with_bounds(cx, show, true, None)
}

#[cfg(feature = "test-support")]
#[test]
fn hidden_window_waits_for_one_explicit_visibility_change() {
    let mut cx = TestAppContext::single();
    let window = open_window(&mut cx, false);

    let initial = cx.window_visibility(window);
    assert!(!initial.is_visible);
    assert_eq!(initial.visibility_change_count, 0);
    assert!(!initial.is_active);
    assert_eq!(initial.activation_change_count, 0);

    window
        .update(&mut cx, |_, window, cx| {
            window.activate_window();
            window.minimize_window();
            window.draw_and_present_for_test(cx);
        })
        .unwrap();
    let prepared = cx.window_visibility(window);
    assert!(!prepared.is_visible);
    assert!(!prepared.is_active);
    assert_eq!(prepared.activation_change_count, 0);
    assert!(!prepared.is_minimized);
    assert_eq!(prepared.minimize_change_count, 0);

    window
        .update(&mut cx, |_, window, cx| window.publish(cx))
        .unwrap()
        .unwrap();
    let published = cx.window_visibility(window);
    assert!(published.is_visible);
    assert_eq!(published.visibility_change_count, 1);
    assert!(!published.is_active);
    assert_eq!(published.activation_change_count, 0);

    window
        .update(&mut cx, |_, window, _| window.activate_window())
        .unwrap();
    let activated = cx.window_visibility(window);
    assert!(activated.is_active);
    assert_eq!(activated.activation_change_count, 1);

    window
        .update(&mut cx, |_, window, _| window.minimize_window())
        .unwrap();
    let minimized = cx.window_visibility(window);
    assert!(minimized.is_minimized);
    assert_eq!(minimized.minimize_change_count, 1);

    window
        .update(&mut cx, |_, window, cx| window.publish(cx))
        .unwrap()
        .unwrap();
    assert_eq!(cx.window_visibility(window).visibility_change_count, 1);
}

#[cfg(feature = "test-support")]
#[test]
fn immediate_window_is_visible_once_at_creation() {
    let mut cx = TestAppContext::single();
    let window = open_window(&mut cx, true);

    let visible = cx.window_visibility(window);
    assert!(visible.is_visible);
    assert_eq!(visible.visibility_change_count, 1);
    assert!(visible.is_active);
    assert_eq!(visible.activation_change_count, 1);

    window
        .update(&mut cx, |_, window, cx| window.publish(cx))
        .unwrap()
        .unwrap();
    assert_eq!(cx.window_visibility(window).visibility_change_count, 1);

    let unfocused = open_window_with_bounds(&mut cx, true, false, None);
    let unfocused_state = cx.window_visibility(unfocused);
    assert!(unfocused_state.is_visible);
    assert!(!unfocused_state.is_active);
    assert_eq!(unfocused_state.activation_change_count, 0);
}

#[cfg(feature = "test-support")]
#[test]
fn failed_visibility_change_remains_retryable() {
    let mut cx = TestAppContext::single();
    let window = open_window(&mut cx, false);
    cx.fail_next_window_visibility_change(window);

    assert!(
        window
            .update(&mut cx, |_, window, cx| window.publish(cx))
            .unwrap()
            .is_err()
    );
    let failed = cx.window_visibility(window);
    assert!(!failed.is_visible);
    assert_eq!(failed.visibility_change_count, 0);

    window
        .update(&mut cx, |_, window, cx| window.publish(cx))
        .unwrap()
        .unwrap();
    assert_eq!(cx.window_visibility(window).visibility_change_count, 1);
}

#[cfg(feature = "test-support")]
#[test]
fn closed_window_handle_cannot_publish() {
    let mut cx = TestAppContext::single();
    let window = open_window(&mut cx, false);

    window
        .update(&mut cx, |_, window, _| window.remove_window())
        .unwrap();
    assert!(
        window
            .update(&mut cx, |_, window, cx| window.publish(cx))
            .is_err()
    );
}

#[cfg(feature = "test-support")]
#[test]
fn removed_window_cannot_publish_in_the_same_update() {
    let mut cx = TestAppContext::single();
    let window = open_window(&mut cx, false);

    assert!(
        window
            .update(&mut cx, |_, window, cx| {
                window.remove_window();
                window.publish(cx)
            })
            .unwrap()
            .is_err()
    );
}

#[cfg(feature = "test-support")]
#[test]
fn hidden_initial_states_are_fixed_until_publication() {
    for window_bounds in [
        WindowBounds::Windowed(Default::default()),
        WindowBounds::Maximized(Default::default()),
        WindowBounds::Fullscreen(Default::default()),
    ] {
        let mut cx = TestAppContext::single();
        let window = open_window_with_bounds(&mut cx, false, true, Some(window_bounds));

        window
            .update(&mut cx, |_, window, _| {
                window.zoom_window();
                window.toggle_fullscreen();
            })
            .unwrap();
        assert_eq!(
            window
                .update(&mut cx, |_, window, _| window.window_bounds())
                .unwrap(),
            window_bounds
        );

        window
            .update(&mut cx, |_, window, cx| window.publish(cx))
            .unwrap()
            .unwrap();
        window
            .update(&mut cx, |_, window, _| window.zoom_window())
            .unwrap();
        let zoomed_bounds = match window_bounds {
            WindowBounds::Maximized(bounds) => WindowBounds::Windowed(bounds),
            WindowBounds::Fullscreen(bounds) | WindowBounds::Windowed(bounds) => {
                WindowBounds::Maximized(bounds)
            }
        };
        assert_eq!(
            window
                .update(&mut cx, |_, window, _| window.window_bounds())
                .unwrap(),
            zoomed_bounds
        );
        window
            .update(&mut cx, |_, window, _| window.toggle_fullscreen())
            .unwrap();
        assert_eq!(
            window
                .update(&mut cx, |_, window, _| window.window_bounds())
                .unwrap(),
            WindowBounds::Fullscreen(window_bounds.get_bounds())
        );
    }
}
