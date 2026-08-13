use super::*;

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
            let error = text_system
                .streaming_layout_session(binding(px(value)))
                .err()
                .unwrap();
            assert_eq!(error, StreamingLayoutError::InvalidMetric(metric));
        }

        let mut invalid_font = binding(px(80.));
        invalid_font.font_size = px(f32::NAN);
        assert_eq!(
            text_system
                .streaming_layout_session(invalid_font)
                .err()
                .unwrap(),
            StreamingLayoutError::InvalidMetric(StreamingLayoutMetric::FontSize)
        );
        let mut invalid_height = binding(px(80.));
        invalid_height.line_height = px(0.);
        assert_eq!(
            text_system
                .streaming_layout_session(invalid_height)
                .err()
                .unwrap(),
            StreamingLayoutError::InvalidMetric(StreamingLayoutMetric::LineHeight)
        );

        for (continuation, metric) in [
            (
                StreamingLayoutContinuation {
                    inline_offset: px(f32::NAN),
                    ..initial_continuation()
                },
                StreamingLayoutMetric::InlineOffset,
            ),
            (
                StreamingLayoutContinuation {
                    block_offset: px(f32::INFINITY),
                    ..initial_continuation()
                },
                StreamingLayoutMetric::BlockOffset,
            ),
            (
                StreamingLayoutContinuation {
                    inline_offset: px(-1.),
                    ..initial_continuation()
                },
                StreamingLayoutMetric::InlineOffset,
            ),
            (
                StreamingLayoutContinuation {
                    line_block_extent: px(f32::NAN),
                    ..initial_continuation()
                },
                StreamingLayoutMetric::LineBlockExtent,
            ),
            (
                StreamingLayoutContinuation {
                    line_block_extent: px(f32::INFINITY),
                    ..initial_continuation()
                },
                StreamingLayoutMetric::LineBlockExtent,
            ),
        ] {
            assert_eq!(
                text_system
                    .resume_streaming_layout_session(binding(px(80.)), continuation)
                    .err()
                    .unwrap(),
                StreamingLayoutError::InvalidMetric(metric)
            );
        }
    });
}

#[gpui::test]
fn invalid_atom_metrics_are_atomic(cx: &mut TestAppContext) {
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
                .streaming_layout_session(binding(px(80.)))
                .unwrap();
            let prior = session.continuation();
            let candidate = StreamingOversizeAtom {
                width: px(width),
                height: px(height),
                baseline: px(baseline),
                ..atom(0, 0..1, "", false)
            };
            assert_eq!(
                session.admit_oversize_atom(candidate).unwrap_err(),
                StreamingLayoutError::InvalidMetric(metric)
            );
            assert_eq!(session.continuation(), prior);
        }
    });
}

#[gpui::test]
fn synthetic_overflow_and_exact_caps_fail_atomically(cx: &mut TestAppContext) {
    assert_eq!(
        StreamingLayoutCharge {
            segment_text: usize::MAX,
            runs: 1,
            ..Default::default()
        }
        .total(),
        Err(StreamingLayoutError::Overflow(
            StreamingLayoutComponent::Total
        ))
    );

    with_text_system(cx, |text_system| {
        let continuation = StreamingLayoutContinuation {
            next_ordinal: u64::MAX,
            ..initial_continuation()
        };
        let mut session = text_system
            .resume_streaming_layout_session(binding(px(80.)), continuation)
            .unwrap();
        let prior = session.continuation();
        let candidate = atom(u64::MAX, 0..1, "", false);
        assert_eq!(
            session.admit_oversize_atom(candidate).unwrap_err(),
            StreamingLayoutError::Overflow(StreamingLayoutComponent::Continuation)
        );
        assert_eq!(session.continuation(), prior);

        let continuation = StreamingLayoutContinuation {
            next_ordinal: 0,
            next_logical_offset: 0,
            inline_offset: px(1.),
            block_offset: px(f32::MAX),
            line_block_extent: px(f32::MAX),
            visual_lines: 0,
        };
        let mut placement_overflow = text_system
            .resume_streaming_layout_session(binding(px(80.)), continuation)
            .unwrap();
        let prior = placement_overflow.continuation();
        assert_eq!(
            placement_overflow
                .admit_oversize_atom(StreamingOversizeAtom {
                    width: px(80.),
                    ..atom(0, 0..1, "", false)
                })
                .unwrap_err(),
            StreamingLayoutError::Overflow(StreamingLayoutComponent::Continuation)
        );
        assert_eq!(placement_overflow.continuation(), prior);

        let mut exact = text_system
            .streaming_layout_session(binding(px(80.)))
            .unwrap();
        let admitted = exact.admit_text(segment(0, 0, "abc", false)).unwrap();
        let exact_total = admitted.charge.total().unwrap();
        let mut exact_binding = binding(px(80.));
        exact_binding.limits.retained_bytes = exact_total;
        text_system
            .streaming_layout_session(exact_binding)
            .unwrap()
            .admit_text(segment(0, 0, "abc", false))
            .unwrap();
    });
}

fn initial_continuation() -> StreamingLayoutContinuation {
    StreamingLayoutContinuation {
        next_ordinal: 0,
        next_logical_offset: 0,
        inline_offset: px(0.),
        block_offset: px(0.),
        line_block_extent: px(14.),
        visual_lines: 0,
    }
}

#[gpui::test]
fn adjacent_multibyte_fragments_have_explicit_boundary_ownership(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(px(120.)))
            .unwrap();
        let first = session.admit_text(segment(0, 0, "é", false)).unwrap();
        let second = session.admit_text(segment(1, 2, "ב", false)).unwrap();
        let StreamingLayoutFragment::Text(first) = &first.fragments[0] else {
            panic!("expected text fragment")
        };
        let StreamingLayoutFragment::Text(second) = &second.fragments[0] else {
            panic!("expected text fragment")
        };

        let first_end = first.position_for_index("é".len()).unwrap().unwrap();
        let second_start = second.position_for_index(0).unwrap().unwrap();
        assert_eq!(first_end, second_start);
        assert_eq!(
            first
                .closest_logical_offset_for_position(first_end)
                .unwrap(),
            StreamingLayoutHit::AfterFragment
        );
        assert_eq!(
            second
                .closest_logical_offset_for_position(second_start)
                .unwrap(),
            StreamingLayoutHit::Offset(2)
        );
        assert_eq!(
            second
                .closest_logical_offset_for_position(point(second_start.x - px(1.), second_start.y))
                .unwrap(),
            StreamingLayoutHit::BeforeFragment
        );
        let second_end = second.position_for_index("ב".len()).unwrap().unwrap();
        assert_eq!(
            second
                .closest_logical_offset_for_position(point(second_end.x + px(1.), second_end.y))
                .unwrap(),
            StreamingLayoutHit::AfterFragment
        );

        let mut reset = text_system
            .streaming_layout_session(binding(px(120.)))
            .unwrap();
        reset.admit_text(segment(0, 0, "é", true)).unwrap();
        let reset_second = reset.admit_text(segment(1, 2, "ב", false)).unwrap();
        let StreamingLayoutFragment::Text(reset_second) = &reset_second.fragments[0] else {
            panic!("expected text fragment")
        };
        let reset_start = reset_second.position_for_index(0).unwrap().unwrap();
        assert_eq!(reset_start.x, px(0.));
        assert_eq!(reset_start.y, px(14.));
    });
}

#[gpui::test]
fn oversize_atom_has_complete_caret_hit_and_adjacent_ownership(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut wide_binding = binding(px(200.));
        wide_binding.limits.segment_bytes = 64;
        let mut session = text_system
            .streaming_layout_session(wide_binding.clone())
            .unwrap();
        let admitted = session
            .admit_oversize_atom(StreamingOversizeAtom {
                width: px(20.),
                ..atom(0, 0..100, "[]", false)
            })
            .unwrap();
        let StreamingLayoutFragment::OversizeAtom(atom_fragment) = &admitted.fragments[0] else {
            panic!("expected atom fragment")
        };
        let left = atom_fragment.position_for_logical_offset(0).unwrap();
        let right = atom_fragment.position_for_logical_offset(100).unwrap();
        assert_eq!(right.x - left.x, px(20.));
        assert_eq!(
            atom_fragment
                .closest_logical_offset_for_position(point(left.x - px(1.), left.y))
                .unwrap(),
            StreamingLayoutHit::BeforeFragment
        );
        assert_eq!(
            atom_fragment
                .closest_logical_offset_for_position(left)
                .unwrap(),
            StreamingLayoutHit::Offset(0)
        );
        assert_eq!(
            atom_fragment
                .closest_logical_offset_for_position(point(left.x + px(9.), left.y))
                .unwrap(),
            StreamingLayoutHit::Offset(0)
        );
        assert_eq!(
            atom_fragment
                .closest_logical_offset_for_position(point(left.x + px(10.), left.y))
                .unwrap(),
            StreamingLayoutHit::AfterFragment
        );
        assert_eq!(
            atom_fragment
                .closest_logical_offset_for_position(right)
                .unwrap(),
            StreamingLayoutHit::AfterFragment
        );

        let following = session.admit_text(segment(1, 100, "x", false)).unwrap();
        let StreamingLayoutFragment::Text(following) = &following.fragments[0] else {
            panic!("expected text fragment")
        };
        let following_start = following.position_for_index(0).unwrap().unwrap();
        assert_eq!(following_start, right);
        assert_eq!(
            following
                .closest_logical_offset_for_position(following_start)
                .unwrap(),
            StreamingLayoutHit::Offset(100)
        );

        let mut terminal = text_system.streaming_layout_session(wide_binding).unwrap();
        let terminal = terminal
            .admit_oversize_atom(StreamingOversizeAtom {
                width: px(20.),
                ..atom(0, 0..100, "[]", true)
            })
            .unwrap();
        let StreamingLayoutFragment::OversizeAtom(terminal) = &terminal.fragments[0] else {
            panic!("expected atom fragment")
        };
        assert_eq!(
            terminal
                .closest_logical_offset_for_position(point(px(10.), px(0.)))
                .unwrap(),
            StreamingLayoutHit::Offset(100)
        );
    });
}

#[gpui::test]
fn rtl_segments_remain_independent_bounded_shaping_contexts(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let first_text = "אבג";
        let second_text = "דהו";
        let second_start = u64::try_from(first_text.len()).unwrap();
        let mut session = text_system
            .streaming_layout_session(binding(px(200.)))
            .unwrap();
        let first = session
            .admit_text(segment(0, 0, first_text, false))
            .unwrap();
        let second = session
            .admit_text(segment(1, second_start, second_text, true))
            .unwrap();
        let StreamingLayoutFragment::Text(first) = &first.fragments[0] else {
            panic!("expected text fragment")
        };
        let StreamingLayoutFragment::Text(second) = &second.fragments[0] else {
            panic!("expected text fragment")
        };

        assert_eq!(first.line().text.as_ref(), first_text);
        assert_eq!(second.line().text.as_ref(), second_text);
        assert!(
            first
                .maps()
                .iter()
                .all(|map| map.logical_offset <= second_start)
        );
        assert!(
            second
                .maps()
                .iter()
                .all(|map| map.logical_offset >= second_start)
        );
        for map in first.maps().iter().chain(second.maps()) {
            assert!(f32::from(map.position.x).is_finite());
            assert!(f32::from(map.position.y).is_finite());
        }
        assert_eq!(first.maps().last().unwrap().logical_offset, second_start);
        assert_eq!(
            second.maps().last().unwrap().logical_offset,
            second_start
                .checked_add(u64::try_from(second_text.len()).unwrap())
                .unwrap()
        );
    });
}

#[path = "corrections/cache_and_extent.rs"]
mod cache_and_extent;
