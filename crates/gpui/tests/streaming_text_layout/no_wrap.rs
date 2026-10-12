use super::*;

#[gpui::test]
fn hundred_kilobyte_line_keeps_one_visual_line_and_bounded_admission(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let input = binding(pos(0), px(1.));
        let chunk = "x".repeat(250);
        let mut session = text_system
            .streaming_layout_session_with_mode(input.clone(), StreamingLayoutMode::NoWrap)
            .unwrap();
        let mut inline = px(0.);
        for ordinal in 0..400 {
            let start = ordinal * 250;
            let admitted = session
                .admit_text(segment(ordinal, pos(start), pos(start + 250), &chunk))
                .unwrap();
            let StreamingLayoutFragment::Text(fragment) = &admitted.fragments[0] else {
                panic!("expected text")
            };
            assert!(fragment.line().wrap_boundaries().is_empty());
            assert_eq!(fragment.maps()[0].position, point(inline, px(0.)));
            assert_eq!(admitted.continuation.visual_lines, 0);
            assert_eq!(admitted.continuation.block_offset, px(0.));
            assert!(admitted.continuation.inline_offset > inline);
            assert!(admitted.charge.total().unwrap() <= input.limits.retained_bytes);
            assert!(admitted.item_charge.total().unwrap() <= input.limits.retained_items);
            inline = admitted.continuation.inline_offset;
            session = text_system
                .resume_streaming_layout_session_with_mode(
                    input.clone(),
                    admitted.continuation,
                    StreamingLayoutMode::NoWrap,
                )
                .unwrap();
        }
        let ended = session
            .end_source(StreamingEndOfSource {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 400,
                source_extent: 100_000,
                position: pos(100_000),
            })
            .unwrap();
        assert_eq!(ended.continuation.mode, StreamingLayoutMode::NoWrap);
        assert_eq!(ended.continuation.visual_lines, 1);
        assert_eq!(ended.continuation.finalized_logical_lines, 1);
        assert!(ended.continuation.ended);
    });
}

#[gpui::test]
fn no_wrap_resume_rejects_both_directions_of_mode_replacement(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let input = binding(pos(0), px(1.));
        for mode in [StreamingLayoutMode::SoftWrap, StreamingLayoutMode::NoWrap] {
            let mut session = text_system
                .streaming_layout_session_with_mode(input.clone(), mode)
                .unwrap();
            let prior = session.continuation();
            let other = if mode == StreamingLayoutMode::NoWrap {
                StreamingLayoutMode::SoftWrap
            } else {
                StreamingLayoutMode::NoWrap
            };
            assert!(matches!(
                text_system.resume_streaming_layout_session_with_mode(
                    input.clone(),
                    prior.unwrap(),
                    other,
                ),
                Err(StreamingLayoutError::InputMismatch)
            ));
            if mode == StreamingLayoutMode::NoWrap {
                assert!(matches!(
                    text_system.resume_streaming_layout_session(input.clone(), prior.unwrap()),
                    Err(StreamingLayoutError::InputMismatch)
                ));
            }
            assert_eq!(session.continuation(), prior);
            session
                .admit_text(segment(0, pos(0), pos(2), "ok"))
                .unwrap();
        }
    });
}

#[gpui::test]
fn no_wrap_atoms_stay_inline_until_explicit_line_finalization(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session_with_mode(
                binding(pos(0), px(1.)),
                StreamingLayoutMode::NoWrap,
            )
            .unwrap();
        let first = session
            .admit_text(segment(0, pos(0), pos(4), "text"))
            .unwrap();
        let oversized = session.admit_oversize_atom(atom(1, 4, 20)).unwrap();
        let StreamingLayoutFragment::OversizeAtom(fragment) = &oversized.fragments[0] else {
            panic!("expected atom")
        };
        assert_eq!(
            fragment.maps()[0].position.x,
            first.continuation.inline_offset
        );
        assert_eq!(fragment.maps()[0].position.y, px(0.));
        assert_eq!(oversized.continuation.visual_lines, 0);
        let finalized = session
            .finalize_logical_line(StreamingLineFinalization {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 2,
                delimiter_range: Some(pos(20)..pos(21)),
                next_position: pos(21),
            })
            .unwrap();
        assert_eq!(finalized.continuation.mode, StreamingLayoutMode::NoWrap);
        assert_eq!(finalized.continuation.inline_offset, px(0.));
        assert_eq!(finalized.continuation.block_offset, px(20.));
        assert_eq!(finalized.continuation.visual_lines, 1);
        let next = session
            .admit_text(segment(3, pos(21), pos(23), "ok"))
            .unwrap();
        let StreamingLayoutFragment::Text(fragment) = &next.fragments[0] else {
            panic!("expected text")
        };
        assert_eq!(fragment.maps()[0].position, point(px(0.), px(20.)));
    });
}

#[gpui::test]
fn no_wrap_same_anchor_objects_keep_exact_inline_gaps(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let first_id = StreamingObjectId(1);
        let second_id = StreamingObjectId(2);
        let first_order = StreamingObjectOrder(10);
        let second_order = StreamingObjectOrder(20);
        let positions = [
            StreamingLayoutPosition::with_gap(0, StreamingObjectGap::before(first_id, first_order)),
            StreamingLayoutPosition::with_gap(
                0,
                StreamingObjectGap::between(first_id, first_order, second_id, second_order),
            ),
            StreamingLayoutPosition::with_gap(
                0,
                StreamingObjectGap::after(second_id, second_order),
            ),
        ];
        let input = binding(positions[0], px(1.));
        let mut session = text_system
            .streaming_layout_session_with_mode(input.clone(), StreamingLayoutMode::NoWrap)
            .unwrap();
        for (ordinal, (id, order)) in [(first_id, first_order), (second_id, second_order)]
            .into_iter()
            .enumerate()
        {
            let admitted = session
                .admit_inline_object(StreamingInlineObject {
                    input_id: 7,
                    segment_policy_id: 11,
                    ordinal: ordinal as u64,
                    id,
                    order,
                    leading: positions[ordinal],
                    trailing: positions[ordinal + 1],
                    presentation: SharedString::new_static("[]"),
                    runs: vec![run(2)],
                    width: px(50.),
                    height: px(20.),
                    baseline: px(12.),
                    background: None,
                })
                .unwrap();
            let StreamingLayoutFragment::InlineObject(fragment) = &admitted.fragments[0] else {
                panic!("expected object")
            };
            assert_eq!(
                fragment.maps()[0].position,
                point(px(ordinal as f32 * 50.), px(0.))
            );
            assert_eq!(admitted.continuation.visual_lines, 0);
            session = text_system
                .resume_streaming_layout_session_with_mode(
                    input.clone(),
                    admitted.continuation,
                    StreamingLayoutMode::NoWrap,
                )
                .unwrap();
        }
        assert_eq!(session.continuation().unwrap().inline_offset, px(100.));
        assert_eq!(session.continuation().unwrap().next_position, positions[2]);
    });
}

#[gpui::test]
fn no_wrap_ignores_wrap_width_but_keeps_atomic_capacity_refusal(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut input = binding(pos(0), px(f32::NAN));
        input.limits.segment_bytes = 3;
        assert!(matches!(
            text_system.streaming_layout_session(input.clone()),
            Err(StreamingLayoutError::InvalidMetric(
                StreamingLayoutMetric::WrapWidth
            ))
        ));
        let mut session = text_system
            .streaming_layout_session_with_mode(input, StreamingLayoutMode::NoWrap)
            .unwrap();
        let prior = session.continuation();
        assert_eq!(
            session
                .admit_text(segment(0, pos(0), pos(4), "long"))
                .unwrap_err(),
            StreamingLayoutError::CapacityExceeded(StreamingLayoutComponent::SegmentText)
        );
        assert_eq!(session.continuation(), prior);
        session
            .admit_text(segment(0, pos(0), pos(2), "ok"))
            .unwrap();
    });
}

#[gpui::test]
fn no_wrap_inline_overflow_keeps_the_prior_continuation(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let input = binding(pos(0), px(1.));
        let origin = text_system
            .streaming_layout_session_with_mode(input.clone(), StreamingLayoutMode::NoWrap)
            .unwrap()
            .continuation()
            .unwrap();
        let prior = StreamingLayoutContinuation {
            next_ordinal: 1,
            next_position: pos(1),
            inline_offset: px(f32::MAX),
            line_has_content: true,
            ..origin
        };
        let mut session = text_system
            .resume_streaming_layout_session_with_mode(input, prior, StreamingLayoutMode::NoWrap)
            .unwrap();
        let mut candidate = atom(1, 1, 2);
        candidate.width = px(f32::MAX);
        assert_eq!(
            session.admit_oversize_atom(candidate).unwrap_err(),
            StreamingLayoutError::Overflow(StreamingLayoutComponent::Continuation)
        );
        assert_eq!(session.continuation(), Some(prior));
    });
}
