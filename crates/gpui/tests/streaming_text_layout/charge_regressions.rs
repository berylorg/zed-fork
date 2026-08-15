use super::*;

#[gpui::test]
fn ordinary_text_charge_is_the_exact_final_graph_breakdown(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut candidate = segment(0, pos(0), pos(6), "abcdef");
        candidate.runs = vec![run(2), run(4)];
        let expected_runs = candidate
            .runs
            .iter()
            .map(|run| std::mem::size_of::<TextRun>() + run.font.family.len())
            .sum::<usize>();
        let mut session = text_system
            .streaming_layout_session(binding(pos(0), px(200.)))
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
                positions: 10,
                gap_witnesses: 10,
                fragments: 1,
                continuations: 1,
                ..Default::default()
            }
        );
        assert_eq!(admitted.item_charge.total(), Ok(40));

        let expected_fragment = std::mem::size_of::<std::ops::Range<StreamingLayoutPosition>>()
            + std::mem::size_of::<gpui::Point<gpui::Pixels>>()
            + 3 * std::mem::size_of::<gpui::Pixels>()
            + std::mem::size_of::<Option<gpui::Pixels>>()
            + 4 * std::mem::size_of::<gpui::Pixels>()
            + std::mem::size_of::<usize>();
        assert_eq!(admitted.charge.segment_text, 6);
        assert_eq!(admitted.charge.runs, expected_runs);
        assert_eq!(
            admitted.charge.decorations,
            std::mem::size_of::<gpui::DecorationRun>()
        );
        assert_eq!(
            admitted.charge.glyphs,
            std::mem::size_of::<gpui::ShapedRun>() + 6 * std::mem::size_of::<gpui::ShapedGlyph>()
        );
        assert_eq!(admitted.charge.wrap_facts, 0);
        assert_eq!(
            admitted.charge.maps,
            7 * std::mem::size_of::<gpui::StreamingLayoutMap>()
        );
        assert_eq!(admitted.charge.objects, 0);
        assert_eq!(admitted.charge.fragments, expected_fragment);
        assert_eq!(
            admitted.charge.continuation,
            std::mem::size_of::<StreamingLayoutContinuation>()
        );
    });
}

#[gpui::test]
fn oversize_atom_charge_counts_exact_retained_graph(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let candidate = atom(0, 0, 10_000);
        let mut session = text_system
            .streaming_layout_session(binding(pos(0), px(200.)))
            .unwrap();
        let admitted = session.admit_oversize_atom(candidate.clone()).unwrap();
        assert_eq!(
            admitted.item_charge,
            StreamingLayoutItemCharge {
                text_payloads: 1,
                shaped_runs: 1,
                glyphs: 6,
                decorations: 1,
                maps: 2,
                positions: 5,
                gap_witnesses: 5,
                fragments: 1,
                continuations: 1,
                ..Default::default()
            }
        );
        assert_eq!(admitted.item_charge.total(), Ok(23));

        let expected_fragment = candidate.presentation.len()
            + std::mem::size_of::<gpui::Bounds<gpui::Pixels>>()
            + std::mem::size_of::<gpui::Pixels>()
            + std::mem::size_of::<Option<gpui::Hsla>>()
            + 4 * std::mem::size_of::<gpui::Pixels>()
            + std::mem::size_of::<usize>()
            + std::mem::size_of::<std::ops::Range<StreamingLayoutPosition>>();
        assert_eq!(admitted.charge.segment_text, 0);
        assert_eq!(admitted.charge.runs, 0);
        assert_eq!(
            admitted.charge.decorations,
            std::mem::size_of::<gpui::DecorationRun>()
        );
        assert_eq!(
            admitted.charge.glyphs,
            std::mem::size_of::<gpui::ShapedRun>() + 6 * std::mem::size_of::<gpui::ShapedGlyph>()
        );
        assert_eq!(admitted.charge.wrap_facts, 0);
        assert_eq!(
            admitted.charge.maps,
            2 * std::mem::size_of::<gpui::StreamingLayoutMap>()
        );
        assert_eq!(admitted.charge.objects, 0);
        assert_eq!(admitted.charge.fragments, expected_fragment);
        assert_eq!(
            admitted.charge.continuation,
            std::mem::size_of::<StreamingLayoutContinuation>()
        );

        let empty = text_system
            .streaming_layout_session(binding(pos(0), px(200.)))
            .unwrap()
            .admit_oversize_atom(StreamingOversizeAtom {
                presentation: SharedString::new_static(""),
                runs: Vec::new(),
                ..atom(0, 0, 10_000)
            })
            .unwrap();
        assert_eq!(
            empty.item_charge,
            StreamingLayoutItemCharge {
                text_payloads: 1,
                maps: 2,
                positions: 5,
                gap_witnesses: 5,
                fragments: 1,
                continuations: 1,
                ..Default::default()
            }
        );
        assert_eq!(empty.item_charge.total(), Ok(15));
    });
}

#[gpui::test]
fn text_and_atom_exact_caps_accept_and_one_under_rejects_atomically(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let text = segment(0, pos(0), pos(3), "abc");
        let admitted = text_system
            .streaming_layout_session(binding(pos(0), px(200.)))
            .unwrap()
            .admit_text(text.clone())
            .unwrap();
        assert_exact_caps(
            text_system,
            binding(pos(0), px(200.)),
            admitted.charge.total().unwrap(),
            admitted.item_charge.total().unwrap(),
            |session| session.admit_text(text.clone()),
        );

        let atom = atom(0, 0, 10);
        let admitted = text_system
            .streaming_layout_session(binding(pos(0), px(200.)))
            .unwrap()
            .admit_oversize_atom(atom.clone())
            .unwrap();
        assert_exact_caps(
            text_system,
            binding(pos(0), px(200.)),
            admitted.charge.total().unwrap(),
            admitted.item_charge.total().unwrap(),
            |session| session.admit_oversize_atom(atom.clone()),
        );
    });
}

fn assert_exact_caps(
    text_system: &WindowTextSystem,
    binding: StreamingLayoutBinding,
    bytes: usize,
    items: usize,
    mut admit: impl FnMut(
        &mut gpui::StreamingLayoutSession<'_>,
    ) -> Result<gpui::StreamingLayoutAdmission, StreamingLayoutError>,
) {
    let mut exact = binding.clone();
    exact.limits.retained_bytes = bytes;
    exact.limits.retained_items = items;
    admit(&mut text_system.streaming_layout_session(exact).unwrap()).unwrap();

    let mut byte_under = binding.clone();
    byte_under.limits.retained_bytes = bytes - 1;
    let mut rejected = text_system.streaming_layout_session(byte_under).unwrap();
    let prior = rejected.continuation();
    assert_eq!(
        admit(&mut rejected).unwrap_err(),
        StreamingLayoutError::CapacityExceeded(StreamingLayoutComponent::Total)
    );
    assert_eq!(rejected.continuation(), prior);

    let mut item_under = binding;
    item_under.limits.retained_items = items - 1;
    let mut rejected = text_system.streaming_layout_session(item_under).unwrap();
    let prior = rejected.continuation();
    assert_eq!(
        admit(&mut rejected).unwrap_err(),
        StreamingLayoutError::CapacityExceeded(StreamingLayoutComponent::Total)
    );
    assert_eq!(rejected.continuation(), prior);
}
