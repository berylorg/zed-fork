use super::*;

#[gpui::test]
fn ordinary_text_item_charge_is_the_exact_final_graph_breakdown(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut candidate = segment(0, 0, "abcdef", false);
        candidate.runs = vec![run(2), run(4)];
        let mut session = text_system
            .streaming_layout_session(binding(px(200.)))
            .unwrap();
        let admitted = session.admit_text(candidate).unwrap();

        assert_eq!(
            admitted.item_charge,
            StreamingLayoutItemCharge {
                text_payloads: 1,
                style_runs: 2,
                shaped_runs: 1,
                glyphs: 6,
                decorations: 1,
                wrap_facts: 0,
                maps: 7,
                fragments: 1,
                continuations: 1,
            }
        );
        assert_eq!(admitted.item_charge.total(), Ok(20));

        let StreamingLayoutFragment::Text(fragment) = &admitted.fragments[0] else {
            panic!("expected text fragment")
        };
        assert_eq!(admitted.item_charge.style_runs, fragment.runs().len());
        assert_eq!(admitted.item_charge.maps, fragment.maps().len());
    });
}

#[gpui::test]
fn item_charge_covers_empty_and_nonempty_wrap_collections(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(px(200.)))
            .unwrap();
        let empty = session
            .admit_text(StreamingTextSegment {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 0,
                logical_range: 0..0,
                next_logical_offset: 1,
                text: SharedString::new_static(""),
                runs: Vec::new(),
                ends_logical_line: true,
            })
            .unwrap();
        assert_eq!(
            empty.item_charge,
            StreamingLayoutItemCharge {
                text_payloads: 1,
                maps: 1,
                fragments: 1,
                continuations: 1,
                ..Default::default()
            }
        );

        let mut wrapped = text_system
            .streaming_layout_session(binding(px(20.)))
            .unwrap();
        let admitted = wrapped
            .admit_text(segment(0, 0, "abcdefgh", false))
            .unwrap();
        let StreamingLayoutFragment::Text(fragment) = &admitted.fragments[0] else {
            panic!("expected text fragment")
        };
        assert!(admitted.item_charge.wrap_facts > 0);
        assert_eq!(
            admitted.item_charge.wrap_facts,
            fragment.line().wrap_boundaries().len()
        );
        assert_eq!(admitted.item_charge.maps, fragment.maps().len());
    });
}

#[gpui::test]
fn oversize_atom_counts_shaped_output_but_not_discarded_caller_runs(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(px(200.)))
            .unwrap();
        let admitted = session
            .admit_oversize_atom(atom(0, 0..10_000, "[item]", false))
            .unwrap();
        assert_eq!(
            admitted.item_charge,
            StreamingLayoutItemCharge {
                text_payloads: 1,
                style_runs: 0,
                shaped_runs: 1,
                glyphs: 6,
                decorations: 1,
                wrap_facts: 0,
                maps: 2,
                fragments: 1,
                continuations: 1,
            }
        );
        assert_eq!(admitted.item_charge.total(), Ok(13));

        let empty = text_system
            .streaming_layout_session(binding(px(200.)))
            .unwrap()
            .admit_oversize_atom(atom(0, 0..10_000, "", false))
            .unwrap();
        assert_eq!(
            empty.item_charge,
            StreamingLayoutItemCharge {
                text_payloads: 1,
                maps: 2,
                fragments: 1,
                continuations: 1,
                ..Default::default()
            }
        );
        assert_eq!(empty.item_charge.total(), Ok(5));
    });
}

#[gpui::test]
fn session_item_charge_is_continuation_only_and_releases_on_cancel(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(px(80.)))
            .unwrap();
        assert_eq!(
            session.retained_item_charge(),
            StreamingLayoutItemCharge {
                continuations: 1,
                ..Default::default()
            }
        );
        session.cancel();
        assert_eq!(session.retained_item_charge(), Default::default());
    });
}

#[test]
fn item_charge_total_is_checked() {
    assert_eq!(
        StreamingLayoutItemCharge {
            text_payloads: usize::MAX,
            style_runs: 1,
            ..Default::default()
        }
        .total(),
        Err(StreamingLayoutError::Overflow(
            StreamingLayoutComponent::Total
        ))
    );
}
