use super::*;

#[gpui::test]
fn streaming_layout_never_populates_the_ordinary_line_cache(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let before = text_system.line_layout_cache_entry_count();
        let mut accepted = text_system
            .streaming_layout_session(binding(pos(0), px(80.)))
            .unwrap();
        accepted
            .admit_text(segment(0, pos(0), pos(22), "unique-stream-accepted"))
            .unwrap();
        assert_eq!(text_system.line_layout_cache_entry_count(), before);

        let mut rejected_binding = binding(pos(0), px(80.));
        rejected_binding.limits.glyphs = 1;
        rejected_binding.limits.retained_items = 4096;
        let mut rejected = text_system
            .streaming_layout_session(rejected_binding)
            .unwrap();
        rejected
            .admit_text(segment(0, pos(0), pos(22), "unique-stream-rejected"))
            .unwrap_err();
        assert_eq!(text_system.line_layout_cache_entry_count(), before);

        let mut atom_session = text_system
            .streaming_layout_session(binding(pos(0), px(100.)))
            .unwrap();
        atom_session
            .admit_oversize_atom(StreamingOversizeAtom {
                presentation: SharedString::new_static("unique-atom"),
                runs: vec![run(11)],
                width: px(90.),
                ..atom(0, 0, 1)
            })
            .unwrap();
        assert_eq!(text_system.line_layout_cache_entry_count(), before);

        let id = StreamingObjectId(1);
        let order = StreamingObjectOrder(1);
        let leading = StreamingLayoutPosition::with_gap(0, StreamingObjectGap::before(id, order));
        let trailing = StreamingLayoutPosition::with_gap(0, StreamingObjectGap::after(id, order));
        text_system
            .streaming_layout_session(binding(leading, px(100.)))
            .unwrap()
            .admit_inline_object(StreamingInlineObject {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 0,
                id,
                order,
                leading,
                trailing,
                presentation: SharedString::new_static("cache-object"),
                runs: vec![run(12)],
                width: px(90.),
                height: px(20.),
                baseline: px(12.),
                background: None,
            })
            .unwrap();
        assert_eq!(text_system.line_layout_cache_entry_count(), before);

        text_system.layout_line("ordinary-cache-entry", px(10.), &[run(20)], None);
        assert!(text_system.line_layout_cache_entry_count() > before);
    });
}

#[gpui::test]
fn adjacent_multibyte_fragments_have_explicit_boundary_ownership(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(pos(0), px(120.)))
            .unwrap();
        let first = session.admit_text(segment(0, pos(0), pos(2), "é")).unwrap();
        let second = session.admit_text(segment(1, pos(2), pos(4), "ב")).unwrap();
        let StreamingLayoutFragment::Text(first) = &first.fragments[0] else {
            panic!("expected text fragment")
        };
        let StreamingLayoutFragment::Text(second) = &second.fragments[0] else {
            panic!("expected text fragment")
        };
        let first_end = first.maps().last().unwrap().position;
        assert_eq!(first.position_for_logical_position(pos(2)).unwrap(), None);
        let second_start = second
            .position_for_logical_position(pos(2))
            .unwrap()
            .unwrap();
        assert_eq!(first_end, second_start);
        assert_eq!(
            first
                .closest_logical_position_for_position(first_end)
                .unwrap(),
            None
        );
        assert_eq!(
            second
                .closest_logical_position_for_position(second_start)
                .unwrap(),
            Some(StreamingLayoutHit::Gap(pos(2)))
        );

        session
            .finalize_logical_line(StreamingLineFinalization {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 2,
                delimiter_range: None,
                next_position: pos(4),
            })
            .unwrap();
        let reset = session.admit_text(segment(3, pos(4), pos(6), "é")).unwrap();
        let StreamingLayoutFragment::Text(reset) = &reset.fragments[0] else {
            panic!("expected text fragment")
        };
        assert_eq!(reset.maps()[0].position, point(px(0.), px(14.)));
    });
}

#[gpui::test]
fn wrapped_following_fragment_owns_the_shared_caret_position(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(pos(0), px(1.)))
            .unwrap();
        let first = session.admit_text(segment(0, pos(0), pos(1), "x")).unwrap();
        let second = session.admit_text(segment(1, pos(1), pos(2), "y")).unwrap();
        let StreamingLayoutFragment::Text(first) = &first.fragments[0] else {
            panic!("expected text fragment")
        };
        let StreamingLayoutFragment::Text(second) = &second.fragments[0] else {
            panic!("expected text fragment")
        };

        assert_eq!(first.position_for_logical_position(pos(1)).unwrap(), None);
        let owned = second
            .position_for_logical_position(pos(1))
            .unwrap()
            .unwrap();
        assert_eq!(owned, second.maps()[0].position);
        assert_eq!(owned.x, px(0.));
        assert_eq!(owned.y, px(14.));
    });
}

#[gpui::test]
fn rtl_segments_remain_independent_bounded_shaping_contexts(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let first_text = "אבג";
        let second_text = "דהו";
        let second_start = first_text.len() as u64;
        let end = second_start + second_text.len() as u64;
        let mut session = text_system
            .streaming_layout_session(binding(pos(0), px(200.)))
            .unwrap();
        let first = session
            .admit_text(segment(0, pos(0), pos(second_start), first_text))
            .unwrap();
        let second = session
            .admit_text(segment(1, pos(second_start), pos(end), second_text))
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
                .all(|map| map.logical_position.byte_offset <= second_start)
        );
        assert!(
            second
                .maps()
                .iter()
                .all(|map| map.logical_position.byte_offset >= second_start)
        );
        for map in first.maps().iter().chain(second.maps()) {
            assert!(f32::from(map.position.x).is_finite());
            assert!(f32::from(map.position.y).is_finite());
        }
        assert_eq!(
            first.maps().last().unwrap().logical_position,
            pos(second_start)
        );
        assert_eq!(second.maps().last().unwrap().logical_position, pos(end));
    });
}

#[gpui::test]
fn oversize_atom_has_complete_caret_hit_and_shared_boundary_ownership(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(pos(0), px(200.)))
            .unwrap();
        let admitted = session
            .admit_oversize_atom(StreamingOversizeAtom {
                width: px(20.),
                ..atom(0, 0, 100)
            })
            .unwrap();
        let StreamingLayoutFragment::OversizeAtom(atom) = &admitted.fragments[0] else {
            panic!("expected atom fragment")
        };
        let left = atom.position_for_logical_position(pos(0)).unwrap();
        let right = atom.maps()[1].position;
        assert_eq!(atom.position_for_logical_position(pos(100)), None);
        assert_eq!(right.x - left.x, px(20.));
        assert_eq!(
            atom.closest_logical_position_for_position(left).unwrap(),
            Some(StreamingLayoutHit::Gap(pos(0)))
        );
        assert_eq!(
            atom.closest_logical_position_for_position(point(left.x + px(9.), left.y))
                .unwrap(),
            Some(StreamingLayoutHit::Gap(pos(0)))
        );
        assert_eq!(
            atom.closest_logical_position_for_position(point(left.x + px(10.), left.y))
                .unwrap(),
            None
        );
        assert_eq!(
            atom.closest_logical_position_for_position(right).unwrap(),
            None
        );

        let following = session
            .admit_text(segment(1, pos(100), pos(101), "x"))
            .unwrap();
        let StreamingLayoutFragment::Text(following) = &following.fragments[0] else {
            panic!("expected text fragment")
        };
        assert_eq!(following.maps()[0].position, right);
        assert_eq!(
            following
                .closest_logical_position_for_position(right)
                .unwrap(),
            Some(StreamingLayoutHit::Gap(pos(100)))
        );

        session
            .finalize_logical_line(StreamingLineFinalization {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 2,
                delimiter_range: None,
                next_position: pos(101),
            })
            .unwrap();
        let eof = session
            .end_source(StreamingEndOfSource {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 3,
                source_extent: 101,
                position: pos(101),
            })
            .unwrap();
        let StreamingLayoutFragment::Boundary(eof) = &eof.fragments[0] else {
            panic!("expected EOF boundary")
        };
        assert_eq!(
            eof.closest_logical_position_for_position(eof.maps()[0].position)
                .unwrap(),
            Some(StreamingLayoutHit::Gap(pos(101)))
        );
    });
}

#[path = "layout_regressions/extents.rs"]
mod extents;
