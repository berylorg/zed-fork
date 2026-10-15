#[cfg(feature = "test-support")]
use gpui::{FontEnvironmentTestContext, TextRun, font, px};

#[test]
fn font_environment_test_binary_builds_without_test_support() {}

#[cfg(feature = "test-support")]
fn run(len: usize) -> TextRun {
    TextRun {
        len,
        font: font("present"),
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

#[cfg(feature = "test-support")]
#[test]
fn keys_are_fixed_size_owner_scoped_and_stable() {
    let first = FontEnvironmentTestContext::new();
    let second = FontEnvironmentTestContext::new();
    let key = first.text_system.font_environment_key().unwrap();
    assert_eq!(std::mem::size_of::<gpui::FontEnvironmentKey>(), 16);
    assert_eq!(Some(key), first.text_system.font_environment_key());
    assert_eq!(Some(key), first.window_text_system().font_environment_key());
    assert_ne!(Some(key), second.text_system.font_environment_key());
    let mut exhausted = FontEnvironmentTestContext::new();
    exhausted.exhaust_owner();
    assert_eq!(exhausted.text_system.font_environment_key(), None);
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn immutable_test_backend_stays_qualified(cx: &mut gpui::TestAppContext) {
    let system = cx.text_system();
    let before = system.font_environment_key().unwrap();
    system.add_fonts(Vec::new()).unwrap();
    system.resolve_font(&font("missing test font"));
    assert_eq!(system.font_environment_key(), Some(before));
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn shared_app_text_system_checks_layout_before_any_window(cx: &mut gpui::TestAppContext) {
    let text_system = cx.text_system();
    let caption = "Pinned controls";
    let runs = [gpui::TextStyle::default().to_run(caption.len())];
    let checked = text_system
        .layout_line_checked(caption, px(13.), &runs, None)
        .unwrap();
    assert!(checked.width > px(0.));
    let forced = text_system
        .layout_line_checked(caption, px(13.), &runs, Some(px(250.)))
        .unwrap();
    assert_eq!(forced.width, checked.width);
    let forced_positions = forced
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.position.x))
        .collect::<Vec<_>>();
    assert!(forced_positions.len() > 1);
    for (index, position) in forced_positions.iter().enumerate() {
        assert_eq!(*position, px(index as f32 * 250.));
    }
    cx.add_empty_window().update(|window, _| {
        let rendered = window
            .text_system()
            .shape_line(caption.into(), px(13.), &runs, None);
        let checked_window = window
            .text_system()
            .layout_line_checked(caption, px(13.), &runs, None)
            .unwrap();
        assert_eq!(checked.width, rendered.width);
        assert_eq!(checked.width, checked_window.width);
        let forced_window = window
            .text_system()
            .layout_line_checked(caption, px(13.), &runs, Some(px(250.)))
            .unwrap();
        assert_eq!(forced_window.width, forced.width);
        assert_eq!(
            forced_window
                .runs
                .iter()
                .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.position.x))
                .collect::<Vec<_>>(),
            forced_positions
        );
    });
}

#[cfg(feature = "test-support")]
#[test]
fn positive_and_negative_requested_fonts_share_one_epoch() {
    let context = FontEnvironmentTestContext::new();
    let present = font("present");
    let missing_a = font("missing A");
    let missing_b = font("missing B");
    let old_id = context.font_id(&present).unwrap();
    for _ in 0..4 {
        assert_eq!(context.font_id(&present).unwrap(), old_id);
        assert!(context.font_id(&missing_a).is_err());
        assert!(context.font_id(&missing_b).is_err());
    }
    assert_eq!(context.counts(), (3, 0));
    let old_key = context.text_system.font_environment_key();
    context
        .text_system
        .add_fonts(vec![std::borrow::Cow::Borrowed(b"virtual font")])
        .unwrap();
    assert_ne!(old_key, context.text_system.font_environment_key());
    assert_ne!(context.font_id(&present).unwrap(), old_id);
    assert!(context.font_id(&missing_a).is_err());
    assert!(context.font_id(&missing_b).is_err());
    assert_eq!(context.counts(), (6, 0));
}

#[cfg(feature = "test-support")]
#[test]
fn resolution_race_refuses_cache_publication() {
    let context = FontEnvironmentTestContext::new();
    let old_key = context.text_system.font_environment_key();
    context.change_during_next_resolution();
    let error = context.font_id(&font("present")).unwrap_err();
    assert_eq!(
        error.downcast_ref::<gpui::FontEnvironmentError>(),
        Some(&gpui::FontEnvironmentError::Changed)
    );
    assert_ne!(old_key, context.text_system.font_environment_key());
    assert_eq!(context.font_id(&font("present")).unwrap(), gpui::FontId(1));
    assert_eq!(context.counts(), (2, 0));
}

#[cfg(feature = "test-support")]
#[test]
fn shape_caches_qualify_epochs_and_suppress_raced_insertion() {
    let context = FontEnvironmentTestContext::new();
    let window = context.window_text_system();
    let runs = [run(3)];
    let old = window.layout_line("abc", px(12.), &runs, None);
    let cached = window.layout_line("abc", px(12.), &runs, None);
    assert!(std::sync::Arc::ptr_eq(&old, &cached));
    assert_eq!(context.counts().1, 1);
    context.set_generation(1);
    let new = window.layout_line("abc", px(12.), &runs, None);
    assert_eq!(new.width, px(11.));
    assert!(!std::sync::Arc::ptr_eq(&old, &new));
    let before = window.line_layout_cache_entry_count();
    context.change_during_next_shape();
    let key = window.font_environment_key();
    let raced = window.layout_line("def", px(12.), &runs, None);
    assert_eq!(raced.width, px(11.));
    assert_ne!(key, window.font_environment_key());
    assert_eq!(window.line_layout_cache_entry_count(), before);
    let successor = window.layout_line("def", px(12.), &runs, None);
    assert_eq!(successor.width, px(12.));
    assert_eq!(context.counts().1, 4);
}

#[cfg(feature = "test-support")]
#[test]
fn unqualified_environment_never_retains_resolution_or_shape_entries() {
    let context = FontEnvironmentTestContext::new();
    let window = context.window_text_system();
    context.make_unqualified();
    assert_eq!(window.font_environment_key(), None);
    for _ in 0..2 {
        assert!(context.font_id(&font("missing A")).is_err());
        window.layout_line("abc", px(12.), &[run(3)], None);
    }
    assert_eq!(context.counts(), (4, 2));
    assert_eq!(window.line_layout_cache_entry_count(), 0);
    let overflow = FontEnvironmentTestContext::new();
    overflow.set_generation(u64::MAX);
    overflow
        .text_system
        .add_fonts(vec![std::borrow::Cow::Borrowed(b"virtual font")])
        .unwrap();
    assert_eq!(overflow.text_system.font_environment_key(), None);
}

#[cfg(feature = "test-support")]
fn binding() -> gpui::StreamingLayoutBinding {
    gpui::StreamingLayoutBinding {
        input_id: 7,
        segment_policy_id: 11,
        start_position: gpui::StreamingLayoutPosition::at(0),
        wrap_width: px(100.),
        font_size: px(12.),
        line_height: px(16.),
        limits: gpui::StreamingLayoutLimits {
            segment_bytes: 256,
            runs: 16,
            decorations: 16,
            glyphs: 256,
            wraps: 64,
            maps: 257,
            fragments: 1,
            retained_items: 4096,
            retained_bytes: 128 * 1024,
        },
    }
}

#[cfg(feature = "test-support")]
#[test]
fn qualified_continuations_refuse_epoch_or_owner_change_and_raced_admission() {
    let context = FontEnvironmentTestContext::new();
    let window = context.window_text_system();
    let mut session = window.streaming_layout_session(binding()).unwrap();
    let prior = session.continuation().unwrap();
    assert_eq!(prior.font_environment, window.font_environment_key());
    let other = FontEnvironmentTestContext::new().window_text_system();
    assert!(matches!(
        other.resume_streaming_layout_session(binding(), prior),
        Err(gpui::StreamingLayoutError::FontEnvironmentChanged)
    ));
    context.change_during_next_shape();
    let rejection = session.admit_text(gpui::StreamingTextSegment {
        input_id: 7,
        segment_policy_id: 11,
        ordinal: 0,
        logical_range: gpui::StreamingLayoutPosition::at(0)..gpui::StreamingLayoutPosition::at(3),
        text: "abc".into(),
        runs: vec![run(3)],
    });
    assert!(matches!(
        rejection,
        Err(gpui::StreamingLayoutError::FontEnvironmentChanged)
    ));
    assert_eq!(session.continuation(), Some(prior));
    assert!(matches!(
        window.resume_streaming_layout_session(binding(), prior),
        Err(gpui::StreamingLayoutError::FontEnvironmentChanged)
    ));
}

#[cfg(feature = "test-support")]
#[test]
fn unqualified_legacy_continuation_remains_explicit_and_resumable() {
    let context = FontEnvironmentTestContext::new();
    context.make_unqualified();
    let window = context.window_text_system();
    let session = window.streaming_layout_session(binding()).unwrap();
    let prior = session.continuation().unwrap();
    assert_eq!(prior.font_environment, None);
    assert!(
        window
            .resume_streaming_layout_session(binding(), prior)
            .is_ok()
    );
}

#[cfg(all(feature = "test-support", target_os = "windows"))]
#[test]
fn native_event_missing_families_adoption_and_old_faces() {
    let context = gpui::WindowsFontEnvironmentTestContext::new().unwrap();
    let diagnostics = context.observation_diagnostics().unwrap();
    println!(
        "Factory5 legacy: {}; Factory7 retained: {}",
        diagnostics.0, diagnostics.1
    );
    let key = context
        .text_system
        .font_environment_key()
        .expect("native Collection3 expiration event unavailable");
    let id = context.text_system.resolve_font(&font(".SystemUIFont"));
    let old_advance = context.old_face_advance(id).unwrap();
    let old_raster = context.old_face_raster(id).unwrap();
    assert!(!old_raster.1.is_empty());
    let window = context.window_text_system();
    let runs = [TextRun {
        font: font(".SystemUIFont"),
        ..run(3)
    }];
    let old_layout = window.layout_line("abc", px(12.), &runs, None);
    context
        .text_system
        .resolve_font(&font("missing Beryl native family A"));
    context
        .text_system
        .resolve_font(&font("missing Beryl native family B"));
    let counts = context.counts();
    for _ in 0..4 {
        context
            .text_system
            .resolve_font(&font("missing Beryl native family A"));
        context
            .text_system
            .resolve_font(&font("missing Beryl native family B"));
        assert_eq!(context.text_system.font_environment_key(), Some(key));
    }
    context.observe_unsignalled_event();
    assert_eq!(context.counts(), counts);
    context.adopt_retained_collection();
    assert_ne!(context.text_system.font_environment_key(), Some(key));
    let successor = context.text_system.resolve_font(&font(".SystemUIFont"));
    assert_ne!(id, successor);
    let new_layout = window.layout_line("abc", px(12.), &runs, None);
    assert!(!std::sync::Arc::ptr_eq(&old_layout, &new_layout));
    assert_eq!(context.old_face_advance(id).unwrap(), old_advance);
    assert_eq!(context.old_face_raster(id).unwrap(), old_raster);
    assert_eq!(context.counts().3, 1);
    assert_eq!(context.refuse_face_at_capacity(id).unwrap().1, 16_384);
    assert_eq!(context.old_face_advance(id).unwrap(), old_advance);
    assert_eq!(context.old_face_raster(id).unwrap(), old_raster);
    context.adopt_retained_collection();
    let callback_failure = context.checked_shape_with_retained_id(id).unwrap_err();
    assert_eq!(
        callback_failure.downcast_ref::<gpui::FontEnvironmentError>(),
        Some(&gpui::FontEnvironmentError::FaceCapacityExceeded)
    );
    let resolution_failure = context
        .text_system
        .try_resolve_font(&font(".SystemUIFont"))
        .unwrap_err();
    assert_eq!(
        resolution_failure.downcast_ref::<gpui::FontEnvironmentError>(),
        Some(&gpui::FontEnvironmentError::FaceCapacityExceeded)
    );
    assert_eq!(context.old_face_advance(id).unwrap(), old_advance);
    let mut session = window.streaming_layout_session(binding()).unwrap();
    let prior = session.continuation();
    let rejected = session.admit_text(gpui::StreamingTextSegment {
        input_id: 7,
        segment_policy_id: 11,
        ordinal: 0,
        logical_range: gpui::StreamingLayoutPosition::at(0)..gpui::StreamingLayoutPosition::at(3),
        text: "abc".into(),
        runs: vec![TextRun {
            font: font(".SystemUIFont"),
            ..run(3)
        }],
    });
    assert!(matches!(
        rejected,
        Err(gpui::StreamingLayoutError::FontFaceCapacityExceeded)
    ));
    assert_eq!(session.continuation(), prior);
}

#[cfg(all(feature = "test-support", target_os = "windows"))]
#[test]
fn native_failed_observation_and_overflow_are_unqualified() {
    let wait = gpui::WindowsFontEnvironmentTestContext::new().unwrap();
    assert!(wait.text_system.font_environment_key().is_some());
    wait.observe_failed_wait();
    assert_eq!(wait.text_system.font_environment_key(), None);
    assert_eq!(wait.counts().3, 0);
    let adoption = gpui::WindowsFontEnvironmentTestContext::new().unwrap();
    adoption.observe_failed_adoption();
    assert_eq!(adoption.text_system.font_environment_key(), None);
    assert_eq!(adoption.counts().3, 1);
    let expired = gpui::WindowsFontEnvironmentTestContext::new().unwrap();
    expired.observe_expired_replacement();
    assert_eq!(expired.text_system.font_environment_key(), None);
    assert_eq!(expired.counts().3, 1);
    let unsupported = gpui::WindowsFontEnvironmentTestContext::new().unwrap();
    unsupported.observe_unobservable_replacement();
    assert_eq!(unsupported.text_system.font_environment_key(), None);
    assert_eq!(unsupported.counts().3, 1);
    adoption
        .text_system
        .resolve_font(&font("missing after failed adoption"));
    assert_eq!(adoption.counts().3, 1);
    let overflow = gpui::WindowsFontEnvironmentTestContext::new().unwrap();
    overflow.exhaust_generation();
    assert_eq!(overflow.text_system.font_environment_key(), None);
}

#[cfg(all(feature = "test-support", target_os = "windows"))]
#[test]
fn native_custom_font_addition_invalidates_resolution_and_shape_epochs() {
    let context = gpui::WindowsFontEnvironmentTestContext::new().unwrap();
    let before = context.text_system.font_environment_key().unwrap();
    let id = context.text_system.resolve_font(&font(".SystemUIFont"));
    let bytes = context.retained_face_bytes(id).unwrap();
    let old_raster = context.old_face_raster(id).unwrap();
    let window = context.window_text_system();
    let runs = [TextRun {
        font: font(".SystemUIFont"),
        ..run(3)
    }];
    let old = window.layout_line("abc", px(12.), &runs, None);
    context
        .text_system
        .resolve_font(&font("missing custom epoch family"));
    context.text_system.add_fonts(Vec::new()).unwrap();
    assert_eq!(context.text_system.font_environment_key(), Some(before));
    context
        .text_system
        .add_fonts(vec![std::borrow::Cow::Owned(bytes)])
        .unwrap();
    assert_ne!(context.text_system.font_environment_key(), Some(before));
    let successor = context.text_system.resolve_font(&font(".SystemUIFont"));
    assert_ne!(successor, id);
    let new = window.layout_line("abc", px(12.), &runs, None);
    assert!(!std::sync::Arc::ptr_eq(&old, &new));
    assert_eq!(context.old_face_raster(id).unwrap(), old_raster);
    assert_eq!(context.counts().3, 0);
}
