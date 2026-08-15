use super::*;

#[gpui::test]
fn style_run_boundaries_must_be_utf8_scalar_boundaries(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut invalid_text = text_system
            .streaming_layout_session(binding(pos(0), px(80.)))
            .unwrap();
        let prior = invalid_text.continuation();
        let mut candidate = segment(0, pos(0), pos(3), "éx");
        candidate.runs = vec![run(1), run(2)];
        assert_eq!(
            invalid_text.admit_text(candidate).unwrap_err(),
            StreamingLayoutError::InvalidSegment
        );
        assert_eq!(invalid_text.continuation(), prior);

        let mut valid_text = text_system
            .streaming_layout_session(binding(pos(0), px(80.)))
            .unwrap();
        let mut candidate = segment(0, pos(0), pos(3), "éx");
        candidate.runs = vec![run(2), run(1)];
        valid_text.admit_text(candidate).unwrap();

        let mut invalid_presentation = text_system
            .streaming_layout_session(binding(pos(0), px(80.)))
            .unwrap();
        let prior = invalid_presentation.continuation();
        let candidate = StreamingOversizeAtom {
            presentation: SharedString::new_static("éx"),
            runs: vec![run(1), run(2)],
            ..atom(0, 0, 1)
        };
        assert_eq!(
            invalid_presentation
                .admit_oversize_atom(candidate)
                .unwrap_err(),
            StreamingLayoutError::InvalidSegment
        );
        assert_eq!(invalid_presentation.continuation(), prior);

        let identity = StreamingObjectId(5);
        let object_order = StreamingObjectOrder(7);
        let leading = StreamingLayoutPosition::with_gap(
            0,
            StreamingObjectGap::before(identity, object_order),
        );
        let trailing =
            StreamingLayoutPosition::with_gap(0, StreamingObjectGap::after(identity, object_order));
        let mut invalid_object = text_system
            .streaming_layout_session(binding(leading, px(80.)))
            .unwrap();
        let prior = invalid_object.continuation();
        let candidate = StreamingInlineObject {
            input_id: 7,
            segment_policy_id: 11,
            ordinal: 0,
            id: identity,
            order: object_order,
            leading,
            trailing,
            presentation: SharedString::new_static("éx"),
            runs: vec![run(1), run(2)],
            width: px(20.),
            height: px(20.),
            baseline: px(12.),
            background: None,
        };
        assert_eq!(
            invalid_object.admit_inline_object(candidate).unwrap_err(),
            StreamingLayoutError::InvalidSegment
        );
        assert_eq!(invalid_object.continuation(), prior);
    });
}

#[gpui::test]
fn style_run_count_cap_rejects_before_boundary_scan(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut capped_binding = binding(pos(0), px(80.));
        capped_binding.limits.runs = 1;

        let mut text_session = text_system
            .streaming_layout_session(capped_binding.clone())
            .unwrap();
        let text_prior = text_session.continuation();
        let mut text = segment(0, pos(0), pos(3), "éx");
        text.runs = vec![run(1), run(2)];
        assert_eq!(
            text_session.admit_text(text).unwrap_err(),
            StreamingLayoutError::CapacityExceeded(StreamingLayoutComponent::Runs)
        );
        assert_eq!(text_session.continuation(), text_prior);

        let mut atom_session = text_system
            .streaming_layout_session(capped_binding.clone())
            .unwrap();
        let atom_prior = atom_session.continuation();
        let atom = StreamingOversizeAtom {
            presentation: SharedString::new_static("éx"),
            runs: vec![run(1), run(2)],
            ..atom(0, 0, 1)
        };
        assert_eq!(
            atom_session.admit_oversize_atom(atom).unwrap_err(),
            StreamingLayoutError::CapacityExceeded(StreamingLayoutComponent::Runs)
        );
        assert_eq!(atom_session.continuation(), atom_prior);

        let identity = StreamingObjectId(13);
        let order = StreamingObjectOrder(17);
        let leading =
            StreamingLayoutPosition::with_gap(0, StreamingObjectGap::before(identity, order));
        let trailing =
            StreamingLayoutPosition::with_gap(0, StreamingObjectGap::after(identity, order));
        capped_binding.start_position = leading;
        let mut object_session = text_system
            .streaming_layout_session(capped_binding)
            .unwrap();
        let object_prior = object_session.continuation();
        let object = StreamingInlineObject {
            input_id: 7,
            segment_policy_id: 11,
            ordinal: 0,
            id: identity,
            order,
            leading,
            trailing,
            presentation: SharedString::new_static("éx"),
            runs: vec![run(1), run(2)],
            width: px(20.),
            height: px(20.),
            baseline: px(12.),
            background: None,
        };
        assert_eq!(
            object_session.admit_inline_object(object).unwrap_err(),
            StreamingLayoutError::CapacityExceeded(StreamingLayoutComponent::Runs)
        );
        assert_eq!(object_session.continuation(), object_prior);
    });
}

#[gpui::test]
fn invalid_binding_and_carried_metrics_are_typed(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        for (value, metric) in [
            (f32::NAN, StreamingLayoutMetric::WrapWidth),
            (f32::INFINITY, StreamingLayoutMetric::WrapWidth),
            (f32::NEG_INFINITY, StreamingLayoutMetric::WrapWidth),
            (-1.0, StreamingLayoutMetric::WrapWidth),
            (0.0, StreamingLayoutMetric::WrapWidth),
        ] {
            assert_eq!(
                text_system
                    .streaming_layout_session(binding(pos(0), px(value)))
                    .err()
                    .unwrap(),
                StreamingLayoutError::InvalidMetric(metric)
            );
        }

        let mut invalid_font = binding(pos(0), px(80.));
        invalid_font.font_size = px(f32::NAN);
        assert_eq!(
            text_system
                .streaming_layout_session(invalid_font)
                .err()
                .unwrap(),
            StreamingLayoutError::InvalidMetric(StreamingLayoutMetric::FontSize)
        );
        let mut invalid_height = binding(pos(0), px(80.));
        invalid_height.line_height = px(0.);
        assert_eq!(
            text_system
                .streaming_layout_session(invalid_height)
                .err()
                .unwrap(),
            StreamingLayoutError::InvalidMetric(StreamingLayoutMetric::LineHeight)
        );

        let initial = text_system
            .streaming_layout_session(binding(pos(0), px(80.)))
            .unwrap()
            .continuation()
            .unwrap();
        for (continuation, metric) in [
            (
                StreamingLayoutContinuation {
                    inline_offset: px(f32::NAN),
                    ..initial
                },
                StreamingLayoutMetric::InlineOffset,
            ),
            (
                StreamingLayoutContinuation {
                    block_offset: px(f32::INFINITY),
                    ..initial
                },
                StreamingLayoutMetric::BlockOffset,
            ),
            (
                StreamingLayoutContinuation {
                    inline_offset: px(-1.),
                    ..initial
                },
                StreamingLayoutMetric::InlineOffset,
            ),
            (
                StreamingLayoutContinuation {
                    line_block_extent: px(f32::NAN),
                    ..initial
                },
                StreamingLayoutMetric::LineBlockExtent,
            ),
            (
                StreamingLayoutContinuation {
                    line_block_extent: px(13.),
                    ..initial
                },
                StreamingLayoutMetric::LineBlockExtent,
            ),
        ] {
            assert_eq!(
                text_system
                    .resume_streaming_layout_session(binding(pos(0), px(80.)), continuation,)
                    .err()
                    .unwrap(),
                StreamingLayoutError::InvalidMetric(metric)
            );
        }
    });
}

#[gpui::test]
fn impossible_public_continuation_phases_are_rejected(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let binding = binding(pos(0), px(80.));
        let initial = text_system
            .streaming_layout_session(binding.clone())
            .unwrap()
            .continuation()
            .unwrap();
        let finalized = StreamingLayoutContinuation {
            next_ordinal: 1,
            visual_lines: 1,
            finalized_logical_lines: 1,
            line_finalized: true,
            ..initial
        };
        assert!(
            text_system
                .resume_streaming_layout_session(binding.clone(), finalized)
                .is_ok()
        );
        assert!(
            text_system
                .resume_streaming_layout_session(
                    binding.clone(),
                    StreamingLayoutContinuation {
                        ended: true,
                        ..finalized
                    },
                )
                .is_ok()
        );
        let cases = [
            (
                StreamingLayoutContinuation {
                    ended: true,
                    ..initial
                },
                StreamingLayoutError::InvalidSegment,
            ),
            (
                StreamingLayoutContinuation {
                    inline_offset: px(1.),
                    ..finalized
                },
                StreamingLayoutError::InvalidMetric(StreamingLayoutMetric::InlineOffset),
            ),
            (
                StreamingLayoutContinuation {
                    line_block_extent: px(15.),
                    ..finalized
                },
                StreamingLayoutError::InvalidMetric(StreamingLayoutMetric::LineBlockExtent),
            ),
            (
                StreamingLayoutContinuation {
                    visual_lines: 1,
                    finalized_logical_lines: 2,
                    ..initial
                },
                StreamingLayoutError::InvalidSegment,
            ),
            (
                StreamingLayoutContinuation {
                    line_finalized: true,
                    ..initial
                },
                StreamingLayoutError::InvalidSegment,
            ),
            (
                StreamingLayoutContinuation {
                    next_ordinal: 1,
                    ..initial
                },
                StreamingLayoutError::InvalidSegment,
            ),
            (
                StreamingLayoutContinuation {
                    line_has_content: true,
                    ..initial
                },
                StreamingLayoutError::InvalidSegment,
            ),
            (
                StreamingLayoutContinuation {
                    next_position: StreamingLayoutPosition::with_gap(
                        0,
                        StreamingObjectGap::before(StreamingObjectId(1), StreamingObjectOrder(1)),
                    ),
                    ended: true,
                    ..finalized
                },
                StreamingLayoutError::InvalidPosition,
            ),
        ];
        for (continuation, expected) in cases {
            assert_eq!(
                text_system
                    .resume_streaming_layout_session(binding.clone(), continuation)
                    .err()
                    .unwrap(),
                expected
            );
        }

        let valid_mid_line = StreamingLayoutContinuation {
            next_ordinal: 9,
            next_position: pos(42),
            inline_offset: px(10.),
            block_offset: px(70.),
            line_block_extent: px(45.),
            line_has_content: true,
            visual_lines: 3,
            finalized_logical_lines: 2,
            ..initial
        };
        assert!(
            text_system
                .resume_streaming_layout_session(binding, valid_mid_line)
                .is_ok()
        );
    });
}

#[gpui::test]
fn invalid_inline_metrics_and_ordinal_overflow_are_atomic(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        for (width, height, baseline, metric) in [
            (f32::NAN, 20., 12., StreamingLayoutMetric::AtomWidth),
            (f32::INFINITY, 20., 12., StreamingLayoutMetric::AtomWidth),
            (-1., 20., 12., StreamingLayoutMetric::AtomWidth),
            (10., 0., 0., StreamingLayoutMetric::AtomHeight),
            (10., f32::NAN, 0., StreamingLayoutMetric::AtomHeight),
            (10., 20., f32::NAN, StreamingLayoutMetric::AtomBaseline),
            (10., 20., -1., StreamingLayoutMetric::AtomBaseline),
            (10., 20., 21., StreamingLayoutMetric::AtomBaseline),
        ] {
            let mut session = text_system
                .streaming_layout_session(binding(pos(0), px(80.)))
                .unwrap();
            let prior = session.continuation();
            let candidate = StreamingOversizeAtom {
                width: px(width),
                height: px(height),
                baseline: px(baseline),
                ..atom(0, 0, 1)
            };
            assert_eq!(
                session.admit_oversize_atom(candidate).unwrap_err(),
                StreamingLayoutError::InvalidMetric(metric)
            );
            assert_eq!(session.continuation(), prior);
        }

        let continuation = StreamingLayoutContinuation {
            next_ordinal: u64::MAX,
            line_has_content: true,
            ..text_system
                .streaming_layout_session(binding(pos(0), px(80.)))
                .unwrap()
                .continuation()
                .unwrap()
        };
        let mut session = text_system
            .resume_streaming_layout_session(binding(pos(0), px(80.)), continuation)
            .unwrap();
        let prior = session.continuation();
        assert_eq!(
            session
                .admit_oversize_atom(atom(u64::MAX, 0, 1))
                .unwrap_err(),
            StreamingLayoutError::Overflow(StreamingLayoutComponent::Continuation)
        );
        assert_eq!(session.continuation(), prior);
    });
}
