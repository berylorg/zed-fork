#![cfg(all(target_os = "windows", feature = "test-support"))]

use gpui::{
    App, AppContext as _, Application, Bounds, Context, FocusHandle, InputHandler, IntoElement,
    Pixels, Point, Render, Styled, Task, UTF16Selection, Window, WindowOptions, canvas,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    cell::{Cell, RefCell},
    ops::Range,
    rc::Rc,
    time::Duration,
};
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::{
        Input::KeyboardAndMouse::{GetActiveWindow, GetFocus, SetActiveWindow, SetFocus},
        WindowsAndMessaging::{IsWindow, SendMessageW, SetForegroundWindow, WM_CHAR},
    },
};

#[derive(Clone)]
struct TextWitness {
    text: Rc<RefCell<String>>,
    replacements: Rc<Cell<usize>>,
    paints: Rc<Cell<usize>>,
    painted_enabled: Rc<Cell<bool>>,
    registrations: Rc<Cell<usize>>,
}

struct NativeEditor {
    enabled: bool,
    focus: FocusHandle,
    witness: TextWitness,
}

impl Render for NativeEditor {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let enabled = self.enabled;
        let focus = self.focus.clone();
        let witness = self.witness.clone();
        canvas(
            |_, _, _| (),
            move |_, _, window, app| {
                witness.paints.set(witness.paints.get() + 1);
                witness.painted_enabled.set(enabled);
                if enabled {
                    window.handle_input(&focus, witness.clone(), app);
                    witness.registrations.set(witness.registrations.get() + 1);
                }
            },
        )
        .size_full()
    }
}

impl InputHandler for TextWitness {
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut App,
    ) -> Option<UTF16Selection> {
        let end = self.text.borrow().encode_utf16().count();
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }

    fn marked_text_range(&mut self, _: &mut Window, _: &mut App) -> Option<Range<usize>> {
        None
    }

    fn text_for_range(
        &mut self,
        range: Range<usize>,
        _: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut App,
    ) -> Option<String> {
        self.text.borrow().get(range).map(str::to_owned)
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        _: &mut App,
    ) {
        assert!(range.is_none());
        self.replacements.set(self.replacements.get() + 1);
        self.text.borrow_mut().push_str(text);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        _: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        _: &mut App,
    ) {
        panic!("CHAR regression must not invoke a composition mutation");
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut App) {}

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: &mut Window,
        _: &mut App,
    ) -> Option<Bounds<Pixels>> {
        None
    }

    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut App,
    ) -> Option<usize> {
        None
    }
}

#[test]
fn native_character_after_enable_and_focus_uses_current_handler_before_next_paint() {
    let witness = TextWitness {
        text: Rc::new(RefCell::new(String::new())),
        replacements: Rc::new(Cell::new(0)),
        paints: Rc::new(Cell::new(0)),
        painted_enabled: Rc::new(Cell::new(false)),
        registrations: Rc::new(Cell::new(0)),
    };
    let observed = witness.clone();
    let completed = Rc::new(Cell::new(false));
    let receipt = completed.clone();
    let timed_out = Rc::new(Cell::new(false));
    let timeout_flag = timed_out.clone();
    let watchdog = Rc::new(RefCell::new(None::<Task<()>>));
    let retained_watchdog = watchdog.clone();

    Application::new().run(move |app| {
        *retained_watchdog.borrow_mut() = Some(app.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_secs(30))
                .await;
            timeout_flag.set(true);
            cx.update(|app| app.quit()).unwrap();
        }));
        let editor_witness = witness.clone();
        let handle = app
            .open_window(WindowOptions::default(), |_, app| {
                app.new(|cx| NativeEditor {
                    enabled: false,
                    focus: cx.focus_handle(),
                    witness: editor_witness,
                })
            })
            .unwrap();
        let raw = handle
            .update(app, |_, window, _| window.window_handle().unwrap().as_raw())
            .unwrap();
        let RawWindowHandle::Win32(win32) = raw else {
            panic!("expected the original Windows native window");
        };
        let hwnd = HWND(win32.hwnd.get() as *mut _);
        assert!(witness.paints.get() > 0);
        assert_eq!(witness.registrations.get(), 0);

        app.spawn(async move |cx| {
            unsafe {
                let _ = SetForegroundWindow(hwnd);
                let _ = SetActiveWindow(hwnd);
                let _ = SetFocus(Some(hwnd));
            }
            assert_eq!(unsafe { GetActiveWindow() }, hwnd);
            assert_eq!(unsafe { GetFocus() }, hwnd);
            cx.update(|app| {
                handle
                    .update(app, |editor, window, cx| {
                        assert_eq!(window.window_handle().unwrap().as_raw(), raw);
                        editor.enabled = true;
                        window.focus(&editor.focus);
                        assert!(editor.focus.is_focused(window));
                        cx.notify();
                    })
                    .unwrap();
            })
            .unwrap();
            assert!(!witness.painted_enabled.get());
            assert_eq!(witness.registrations.get(), 0);
            unsafe { SendMessageW(hwnd, WM_CHAR, Some(WPARAM('!' as usize)), Some(LPARAM(0))) };
            cx.update(|app| {
                handle
                    .update(app, |editor, window, _| {
                        assert!(editor.enabled);
                        assert!(editor.focus.is_focused(window));
                        assert_eq!(window.window_handle().unwrap().as_raw(), raw);
                        assert!(unsafe { IsWindow(Some(hwnd)) }.as_bool());
                        assert_eq!(unsafe { GetActiveWindow() }, hwnd);
                        assert_eq!(unsafe { GetFocus() }, hwnd);
                        window.remove_window();
                    })
                    .unwrap();
            })
            .unwrap();
            receipt.set(true);
        })
        .detach();
    });

    drop(watchdog.borrow_mut().take());
    assert!(
        !timed_out.get(),
        "native character delivery exceeded App watchdog"
    );
    assert!(completed.get());
    assert_eq!(observed.replacements.get(), 1);
    assert_eq!(&*observed.text.borrow(), "!");
    assert!(observed.painted_enabled.get());
    assert!(observed.registrations.get() > 0);
}

use gpui::ParentElement as _;

#[derive(Default)]
struct NativeInputFacts {
    text: [String; 2],
    replacements: [Vec<String>; 2],
    marks: [usize; 2],
    selected: [Vec<Range<usize>>; 2],
    bounds: [Vec<(Range<usize>, Bounds<Pixels>)>; 2],
    composition_queries: [usize; 2],
    keys: [Vec<String>; 2],
    key_ups: [Vec<String>; 2],
    registrations: [usize; 2],
}

struct NativeInputState {
    facts: RefCell<NativeInputFacts>,
    nested_character: Cell<bool>,
    hwnd: Cell<HWND>,
}

#[derive(Clone)]
struct NativeInputHandler {
    id: usize,
    state: Rc<NativeInputState>,
}

impl InputHandler for NativeInputHandler {
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut App,
    ) -> Option<UTF16Selection> {
        let mut facts = self.state.facts.borrow_mut();
        let end = facts.text[self.id].encode_utf16().count();
        facts.selected[self.id].push(end..end);
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }

    fn marked_text_range(&mut self, _: &mut Window, _: &mut App) -> Option<Range<usize>> {
        let mut facts = self.state.facts.borrow_mut();
        facts.composition_queries[self.id] += 1;
        (facts.marks[self.id] > 0).then(|| 0..facts.text[self.id].encode_utf16().count())
    }

    fn text_for_range(
        &mut self,
        range: Range<usize>,
        _: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut App,
    ) -> Option<String> {
        self.state.facts.borrow().text[self.id]
            .get(range)
            .map(str::to_owned)
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        _: &mut App,
    ) {
        assert!(range.is_none());
        {
            let mut facts = self.state.facts.borrow_mut();
            facts.replacements[self.id].push(text.to_owned());
            facts.text[self.id].push_str(text);
        }
        if self.state.nested_character.replace(false) {
            unsafe {
                SendMessageW(
                    self.state.hwnd.get(),
                    WM_CHAR,
                    Some(WPARAM('?' as usize)),
                    Some(LPARAM(0)),
                )
            };
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        _: &mut App,
    ) {
        assert!(range.is_none());
        assert!(selected.is_none());
        let mut facts = self.state.facts.borrow_mut();
        facts.marks[self.id] += 1;
        facts.text[self.id].push_str(text);
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut App) {
        self.state.facts.borrow_mut().marks[self.id] = 0;
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        _: &mut App,
    ) -> Option<Bounds<Pixels>> {
        let bounds = gpui::Bounds::new(
            gpui::point(
                gpui::px(10. + self.id as f32 * 10.),
                gpui::px(20. + self.id as f32 * 10.),
            ),
            gpui::size(gpui::px(30.), gpui::px(40.)),
        );
        self.state.facts.borrow_mut().bounds[self.id].push((range, bounds));
        Some(bounds)
    }

    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut App,
    ) -> Option<usize> {
        None
    }
}

struct NativeEditors {
    enabled: [bool; 2],
    focus: [FocusHandle; 2],
    selected: usize,
    state: Rc<NativeInputState>,
}

impl Render for NativeEditors {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        use gpui::InteractiveElement as _;
        let enabled = self.enabled;
        let focus = self.focus.clone();
        let selected = self.selected;
        let input_state = self.state.clone();
        let key_state = self.state.clone();
        let key_up_state = self.state.clone();
        gpui::div()
            .size_full()
            .track_focus(&self.focus[selected])
            .on_key_down(move |event, _, _| {
                key_state.facts.borrow_mut().keys[selected].push(event.keystroke.key.clone());
            })
            .on_key_up(move |event, _, _| {
                key_up_state.facts.borrow_mut().key_ups[selected].push(event.keystroke.key.clone());
            })
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, app| {
                        for id in 0..2 {
                            if enabled[id] {
                                window.handle_input(
                                    &focus[id],
                                    NativeInputHandler {
                                        id,
                                        state: input_state.clone(),
                                    },
                                    app,
                                );
                                if focus[id].is_focused(window) {
                                    input_state.facts.borrow_mut().registrations[id] += 1;
                                }
                            }
                        }
                    },
                )
                .size_full(),
            )
    }
}

#[derive(Clone, Copy)]
enum NativeAccessCase {
    Switch,
    Disabled,
    Hidden,
    Closed,
    Nested,
    DeadCharacter,
    Composition,
    Geometry,
    Key,
    KeyUp,
}

fn verify_native_access(case: NativeAccessCase) {
    use windows::Win32::UI::{
        Input::KeyboardAndMouse::VK_F6,
        WindowsAndMessaging::{
            IsWindowVisible, WM_DEADCHAR, WM_IME_COMPOSITION, WM_IME_STARTCOMPOSITION, WM_KEYDOWN,
            WM_KEYUP,
        },
    };
    let state = Rc::new(NativeInputState {
        facts: RefCell::new(NativeInputFacts {
            text: ["aa".into(), "bbb".into()],
            ..Default::default()
        }),
        nested_character: Cell::new(matches!(case, NativeAccessCase::Nested)),
        hwnd: Cell::new(HWND::default()),
    });
    let observed = state.clone();
    let completed = Rc::new(Cell::new(false));
    let receipt = completed.clone();
    let timed_out = Rc::new(Cell::new(false));
    let timeout_flag = timed_out.clone();
    let watchdog = Rc::new(RefCell::new(None::<Task<()>>));
    let retained_watchdog = watchdog.clone();

    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            *retained_watchdog.borrow_mut() = Some(app.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_secs(30))
                    .await;
                timeout_flag.set(true);
                cx.update(|app| app.quit()).unwrap();
            }));
            let editor_state = state.clone();
            let handle = app
                .open_window(
                    WindowOptions {
                        show: !matches!(case, NativeAccessCase::Hidden),
                        focus: !matches!(case, NativeAccessCase::Hidden),
                        ..Default::default()
                    },
                    |window, app| {
                        app.new(|cx| {
                            let focus = [cx.focus_handle(), cx.focus_handle()];
                            window.focus(&focus[0]);
                            NativeEditors {
                                enabled: [true, true],
                                focus,
                                selected: 0,
                                state: editor_state,
                            }
                        })
                    },
                )
                .unwrap();
            let raw = handle
                .update(app, |_, window, _| window.window_handle().unwrap().as_raw())
                .unwrap();
            let RawWindowHandle::Win32(win32) = raw else {
                panic!("expected original native Windows owner");
            };
            let hwnd = HWND(win32.hwnd.get() as *mut _);
            state.hwnd.set(hwnd);
            assert!(state.facts.borrow().registrations[0] > 0);
            assert_eq!(state.facts.borrow().registrations[1], 0);
            app.spawn(async move |cx| {
                let original_active = unsafe { GetActiveWindow() };
                let original_focus = unsafe { GetFocus() };
                if !matches!(case, NativeAccessCase::Hidden) {
                    unsafe {
                        let _ = SetForegroundWindow(hwnd);
                        let _ = SetActiveWindow(hwnd);
                        let _ = SetFocus(Some(hwnd));
                    }
                    assert_eq!(unsafe { GetActiveWindow() }, hwnd);
                    assert_eq!(unsafe { GetFocus() }, hwnd);
                }
                {
                    let mut facts = state.facts.borrow_mut();
                    facts.selected.iter_mut().for_each(Vec::clear);
                    facts.bounds.iter_mut().for_each(Vec::clear);
                    facts.composition_queries = [0, 0];
                    facts.keys.iter_mut().for_each(Vec::clear);
                    facts.key_ups.iter_mut().for_each(Vec::clear);
                }
                cx.update(|app| {
                    handle
                        .update(app, |editor, window, cx| {
                            assert_eq!(window.window_handle().unwrap().as_raw(), raw);
                            if matches!(
                                case,
                                NativeAccessCase::Switch
                                    | NativeAccessCase::DeadCharacter
                                    | NativeAccessCase::Composition
                                    | NativeAccessCase::Geometry
                                    | NativeAccessCase::Key
                                    | NativeAccessCase::KeyUp
                            ) {
                                editor.selected = 1;
                                window.focus(&editor.focus[1]);
                                cx.notify();
                            } else if matches!(case, NativeAccessCase::Disabled) {
                                editor.enabled = [false, false];
                                cx.notify();
                            }
                            if matches!(case, NativeAccessCase::Hidden) {
                                assert!(editor.enabled[0]);
                                assert!(editor.focus[0].is_focused(window));
                                assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());
                            }
                        })
                        .unwrap();
                })
                .unwrap();
                assert_eq!(state.facts.borrow().registrations[1], 0);
                if !matches!(case, NativeAccessCase::Closed) {
                    let (message, character, parameter) = match case {
                        NativeAccessCase::DeadCharacter => (WM_DEADCHAR, '~' as usize, 0),
                        NativeAccessCase::Composition => (WM_IME_COMPOSITION, 0, 0),
                        NativeAccessCase::Geometry => (WM_IME_STARTCOMPOSITION, 0, 0),
                        NativeAccessCase::Key => (WM_KEYDOWN, VK_F6.0 as usize, 1),
                        NativeAccessCase::KeyUp => (WM_KEYUP, VK_F6.0 as usize, 1isize << 31),
                        _ => (WM_CHAR, '!' as usize, 0),
                    };
                    unsafe {
                        SendMessageW(
                            hwnd,
                            message,
                            Some(WPARAM(character)),
                            Some(LPARAM(parameter)),
                        )
                    };
                }
                cx.update(|app| {
                    handle
                        .update(app, |_, window, _| {
                            assert_eq!(window.window_handle().unwrap().as_raw(), raw);
                            if matches!(case, NativeAccessCase::Hidden) {
                                assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());
                                assert_eq!(unsafe { GetActiveWindow() }, original_active);
                                assert_eq!(unsafe { GetFocus() }, original_focus);
                            }
                            window.remove_window();
                        })
                        .unwrap();
                    app.quit();
                })
                .unwrap();
                receipt.set(true);
            })
            .detach();
        });

    drop(watchdog.borrow_mut().take());
    assert!(
        !timed_out.get(),
        "native input access exceeded App watchdog"
    );
    assert!(completed.get());
    let hwnd = observed.hwnd.get();
    assert!(!unsafe { IsWindow(Some(hwnd)) }.as_bool());
    if matches!(case, NativeAccessCase::Closed) {
        unsafe { SendMessageW(hwnd, WM_CHAR, Some(WPARAM('!' as usize)), Some(LPARAM(0))) };
    }
    let facts = observed.facts.borrow();
    match case {
        NativeAccessCase::Switch => {
            assert_eq!(facts.text, ["aa", "bbb!"]);
            assert!(facts.replacements[0].is_empty());
            assert_eq!(facts.replacements[1], ["!"]);
        }
        NativeAccessCase::Nested => {
            assert!(!observed.nested_character.get());
            assert_eq!(facts.text, ["aa!", "bbb"]);
            assert_eq!(facts.replacements[0], ["!"]);
            assert!(facts.replacements[1].is_empty());
        }
        NativeAccessCase::Disabled | NativeAccessCase::Hidden | NativeAccessCase::Closed => {
            assert_eq!(facts.text, ["aa", "bbb"]);
            assert!(facts.replacements.iter().all(Vec::is_empty));
        }
        NativeAccessCase::DeadCharacter => {
            assert_eq!(facts.text, ["aa", "bbb~"]);
            assert_eq!(facts.marks, [0, 1]);
        }
        NativeAccessCase::Composition => {
            assert_eq!(facts.text, ["aa", "bbb"]);
            assert!(facts.replacements[0].is_empty());
            assert_eq!(facts.replacements[1], [""]);
        }
        NativeAccessCase::Geometry => {
            assert_eq!(facts.selected[1], [3..3]);
            assert!(facts.selected[0].is_empty());
            assert_eq!(
                facts.bounds[1],
                [(
                    3..3,
                    Bounds::new(
                        gpui::point(gpui::px(20.), gpui::px(30.)),
                        gpui::size(gpui::px(30.), gpui::px(40.))
                    )
                )]
            );
            assert!(facts.bounds[0].is_empty());
            assert_eq!(facts.text, ["aa", "bbb"]);
        }
        NativeAccessCase::Key => {
            assert!(facts.keys[0].is_empty());
            assert_eq!(facts.keys[1], ["f6"]);
            assert_eq!(facts.composition_queries, [0, 1]);
            assert_eq!(facts.text, ["aa", "bbb"]);
        }
        NativeAccessCase::KeyUp => {
            assert!(facts.key_ups[0].is_empty());
            assert_eq!(facts.key_ups[1], ["f6"]);
            assert_eq!(facts.composition_queries, [0, 0]);
            assert_eq!(facts.text, ["aa", "bbb"]);
        }
    }
}

#[test]
fn native_character_after_focus_switch_reaches_only_current_editor() {
    verify_native_access(NativeAccessCase::Switch);
}
#[test]
fn native_character_after_disable_has_no_eligible_handler() {
    verify_native_access(NativeAccessCase::Disabled);
}
#[test]
fn native_character_never_published_owner_refuses_focused_enabled_handler() {
    verify_native_access(NativeAccessCase::Hidden);
}
#[test]
fn native_character_after_owner_closed_does_not_mutate_or_recreate() {
    verify_native_access(NativeAccessCase::Closed);
}
#[test]
fn native_nested_character_is_refused_while_original_handler_is_in_use() {
    verify_native_access(NativeAccessCase::Nested);
}
#[test]
fn native_dead_character_uses_current_focused_handler() {
    verify_native_access(NativeAccessCase::DeadCharacter);
}
#[test]
fn native_empty_ime_composition_observes_current_handler_operation() {
    verify_native_access(NativeAccessCase::Composition);
}
#[test]
fn native_ime_geometry_queries_current_selection_and_bounds() {
    verify_native_access(NativeAccessCase::Geometry);
}
#[test]
fn native_key_after_dirty_focus_uses_current_dispatch_and_composition_handler() {
    verify_native_access(NativeAccessCase::Key);
}

#[test]
fn native_key_up_after_dirty_focus_synchronizes_dispatch_without_text_access() {
    verify_native_access(NativeAccessCase::KeyUp);
}
