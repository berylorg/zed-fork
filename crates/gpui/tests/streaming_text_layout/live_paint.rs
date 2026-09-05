use super::*;
use gpui::{
    App, AppContext, Context, Hsla, IntoElement, Render, StrikethroughStyle, Styled,
    UnderlineStyle, Window, canvas, rgb, test::PaintSnapshot,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

type PaintTest = Box<dyn FnOnce(&mut Window, &mut App)>;

struct PaintFixture(Rc<RefCell<Option<PaintTest>>>);

impl Render for PaintFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let test = self.0.clone();
        canvas(
            |_, _, _| (),
            move |_, _, window, cx| {
                if let Some(test) = test.borrow_mut().take() {
                    let glyphs = window.text_system().shape_line(
                        "a😀b".into(),
                        px(10.),
                        &[run("a😀b".len())],
                        None,
                    );
                    window.text_system().set_glyph_raster_bounds_for_test(
                        &glyphs,
                        window.scale_factor(),
                        gpui::Bounds::new(
                            point(gpui::DevicePixels(0), gpui::DevicePixels(-6)),
                            gpui::size(gpui::DevicePixels(4), gpui::DevicePixels(6)),
                        ),
                    );
                    test(window, cx);
                }
            },
        )
        .size_full()
    }
}

fn with_paint(cx: &mut TestAppContext, test: impl FnOnce(&mut Window, &mut App) + 'static) {
    let test = Rc::new(RefCell::new(Some(Box::new(test) as PaintTest)));
    let window = cx.add_window(|_, _| PaintFixture(test.clone()));
    cx.update_window(window.into(), |_, window, cx| {
        window.draw_and_present_for_test(cx);
    })
    .unwrap();
    assert!(test.borrow().is_none(), "paint fixture must execute");
}

fn capture(window: &mut Window, paint: impl FnOnce(&mut Window)) -> PaintSnapshot {
    let before = window.paint_snapshot_for_test();
    paint(window);
    let mut after = window.paint_snapshot_for_test();
    after.glyphs.drain(..before.glyphs.len());
    after.decorations.drain(..before.decorations.len());
    after.backgrounds.drain(..before.backgrounds.len());
    after.color_glyphs.drain(..before.color_glyphs.len());
    after
}

fn stored() -> Hsla {
    rgb(0xc03020).into()
}

fn replacement() -> Hsla {
    rgb(0x20b070).into()
}

fn explicit() -> Hsla {
    rgb(0x3050d0).into()
}

fn decorated_runs(len: usize) -> Vec<TextRun> {
    let mut inherited = run(len / 2);
    inherited.color = stored();
    inherited.background_color = Some(rgb(0xe0d0c0).into());
    inherited.underline = Some(UnderlineStyle {
        thickness: px(1.),
        color: None,
        wavy: true,
    });
    inherited.strikethrough = Some(StrikethroughStyle {
        thickness: px(2.),
        color: None,
    });
    let mut colored = inherited.clone();
    colored.len = len - inherited.len;
    colored.color = rgb(0xb030a0).into();
    colored.underline.as_mut().unwrap().color = Some(explicit());
    colored.strikethrough.as_mut().unwrap().color = Some(explicit());
    vec![inherited, colored]
}

fn assert_recolored(default: &PaintSnapshot, changed: &PaintSnapshot) {
    assert!(!default.glyphs.is_empty());
    assert_eq!(default.glyphs.len(), changed.glyphs.len());
    assert!(default.glyphs.iter().any(|(_, color)| *color == stored()));
    for ((old_bounds, _), (new_bounds, color)) in default.glyphs.iter().zip(&changed.glyphs) {
        assert_eq!(old_bounds, new_bounds);
        assert_eq!(*color, replacement());
    }
    assert!(!default.decorations.is_empty());
    assert_eq!(default.decorations.len(), changed.decorations.len());
    assert!(default.decorations.iter().any(|line| line.1 == stored()));
    assert!(
        default.decorations.iter().any(|line| line.1 == explicit()),
        "{default:?}"
    );
    for (old, new) in default.decorations.iter().zip(&changed.decorations) {
        assert_eq!((old.0, old.2, old.3), (new.0, new.2, new.3));
        assert_eq!(
            new.1,
            if old.1 == explicit() {
                explicit()
            } else {
                replacement()
            }
        );
    }
    assert_eq!(default.backgrounds, changed.backgrounds);
    assert_eq!(default.color_glyphs, changed.color_glyphs);
}

#[gpui::test]
fn text_live_paint_preserves_wrapping_maps_accounting_and_stored_colors(cx: &mut TestAppContext) {
    with_paint(cx, |window, cx| {
        let text_system = window.text_system().clone();
        let mut session = text_system
            .streaming_layout_session(binding(pos(0), px(40.)))
            .unwrap();
        let mut prefix = atom(0, 0, 100);
        prefix.presentation = "".into();
        prefix.runs.clear();
        prefix.width = px(12.);
        session.admit_oversize_atom(prefix).unwrap();
        let mut text = segment(1, pos(100), pos(111), "ab cd ef gh");
        text.runs = decorated_runs(text.text.len());
        let admission = session.admit_text(text).unwrap();
        let StreamingLayoutFragment::Text(fragment) = &admission.fragments[0] else {
            panic!("expected text")
        };
        assert!(!fragment.line().wrap_boundaries().is_empty());
        let line = fragment.line().clone();
        let maps = fragment.maps().to_vec();
        let positions: Vec<_> = maps
            .iter()
            .map(|map| {
                (
                    fragment
                        .position_for_logical_position(map.logical_position)
                        .unwrap(),
                    fragment
                        .closest_logical_position_for_position(map.position)
                        .unwrap(),
                )
            })
            .collect();
        let origin = point(px(10.), px(10.));
        let before = format!("{admission:?}");
        let charge = session.retained_charge();
        let items = session.retained_item_charge();
        let continuation = session.continuation().unwrap();
        let paint_default = |window: &mut Window, cx: &mut App| {
            capture(window, |window| {
                fragment.paint_background(origin, window, cx).unwrap();
                fragment.paint(origin, window, cx).unwrap();
            })
        };
        let default = paint_default(window, cx);
        let changed = capture(window, |window| {
            fragment.paint_background(origin, window, cx).unwrap();
            fragment
                .paint_with_color(origin, replacement(), window, cx)
                .unwrap();
        });
        assert_recolored(&default, &changed);
        assert!(!default.backgrounds.is_empty());
        assert_eq!(default, paint_default(window, cx));
        assert!(Arc::ptr_eq(&line, fragment.line()));
        assert_eq!(fragment.maps(), maps);
        assert_eq!(before, format!("{admission:?}"));
        assert_eq!(session.retained_charge(), charge);
        assert_eq!(session.retained_item_charge(), items);
        assert_eq!(session.continuation().unwrap(), continuation);
        for (map, (caret, hit)) in maps.into_iter().zip(positions) {
            assert_eq!(
                fragment
                    .position_for_logical_position(map.logical_position)
                    .unwrap(),
                caret
            );
            assert_eq!(
                fragment
                    .closest_logical_position_for_position(map.position)
                    .unwrap(),
                hit
            );
        }
    });
}

#[gpui::test]
fn inline_live_paint_replaces_or_removes_background_without_mutating_fragments(
    cx: &mut TestAppContext,
) {
    with_paint(cx, |window, cx| {
        let text_system = window.text_system().clone();
        let mut source_atom = atom(0, 0, 100);
        let mut inline_run = decorated_runs(2).remove(0);
        inline_run.strikethrough.as_mut().unwrap().color = Some(explicit());
        source_atom.presentation = "X".into();
        source_atom.runs = vec![inline_run.clone()];
        let admitted_atom = text_system
            .streaming_layout_session(binding(pos(0), px(100.)))
            .unwrap()
            .admit_oversize_atom(source_atom)
            .unwrap();
        let id = StreamingObjectId(5);
        let order = StreamingObjectOrder(10);
        let leading = StreamingLayoutPosition::with_gap(0, StreamingObjectGap::before(id, order));
        let trailing = StreamingLayoutPosition::with_gap(0, StreamingObjectGap::after(id, order));
        let admitted_object = text_system
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
                presentation: "X".into(),
                runs: vec![inline_run],
                width: px(90.),
                height: px(20.),
                baseline: px(12.),
                background: Some(gpui::black()),
            })
            .unwrap();
        let origin = point(px(10.), px(10.));
        for admission in [&admitted_atom, &admitted_object] {
            let before = format!("{admission:?}");
            let paint = |color: Option<Hsla>,
                         background: Option<Option<Hsla>>,
                         window: &mut Window,
                         cx: &mut App| {
                capture(window, |window| {
                    macro_rules! paint_fragment {
                        ($fragment:expr) => {{
                            match background {
                                Some(color) => $fragment
                                    .paint_background_with_color(origin, color, window)
                                    .unwrap(),
                                None => $fragment.paint_background(origin, window).unwrap(),
                            }
                            match color {
                                Some(color) => $fragment
                                    .paint_with_color(origin, color, window, cx)
                                    .unwrap(),
                                None => $fragment.paint(origin, window, cx).unwrap(),
                            }
                        }};
                    }
                    match &admission.fragments[0] {
                        StreamingLayoutFragment::OversizeAtom(fragment) => {
                            paint_fragment!(fragment)
                        }
                        StreamingLayoutFragment::InlineObject(fragment) => {
                            paint_fragment!(fragment)
                        }
                        _ => panic!("expected inline fragment"),
                    }
                })
            };
            let default = paint(None, None, window, cx);
            let changed = paint(Some(replacement()), None, window, cx);
            assert_recolored(&default, &changed);
            assert_eq!(default.backgrounds.len(), 1);
            assert_eq!(default.backgrounds[0].1, gpui::black().into());
            let recolored_background =
                paint(Some(replacement()), Some(Some(explicit())), window, cx);
            assert_eq!(
                recolored_background.backgrounds,
                vec![(default.backgrounds[0].0, explicit().into())]
            );
            assert_eq!(changed.glyphs, recolored_background.glyphs);
            let removed_background = paint(Some(replacement()), Some(None), window, cx);
            assert!(removed_background.backgrounds.is_empty());
            assert_eq!(changed.glyphs, removed_background.glyphs);
            assert_eq!(changed.decorations, removed_background.decorations);
            assert_eq!(default, paint(None, None, window, cx));
            assert_eq!(before, format!("{admission:?}"));
        }
    });
}

#[gpui::test]
fn text_live_paint_retains_color_emoji(cx: &mut TestAppContext) {
    with_paint(cx, |window, cx| {
        let text = "a😀b";
        let admission = window
            .text_system()
            .streaming_layout_session(binding(pos(0), px(100.)))
            .unwrap()
            .admit_text(segment(0, pos(0), pos(text.len() as u64), text))
            .unwrap();
        let StreamingLayoutFragment::Text(fragment) = &admission.fragments[0] else {
            panic!("expected text")
        };
        let origin = point(px(10.), px(10.));
        let default = capture(window, |window| fragment.paint(origin, window, cx).unwrap());
        let changed = capture(window, |window| {
            fragment
                .paint_with_color(origin, replacement(), window, cx)
                .unwrap()
        });
        assert!(!default.glyphs.is_empty());
        assert!(!default.color_glyphs.is_empty());
        assert_eq!(default.color_glyphs, changed.color_glyphs);
        assert_eq!(default.glyphs.len(), changed.glyphs.len());
        assert!(
            changed
                .glyphs
                .iter()
                .all(|(_, color)| *color == replacement())
        );
    });
}
