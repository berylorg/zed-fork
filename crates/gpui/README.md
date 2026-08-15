# Welcome to GPUI!

GPUI is a hybrid immediate and retained mode, GPU accelerated, UI framework
for Rust, designed to support a wide variety of applications.

## Getting Started

GPUI is still in active development as we work on the Zed code editor, and is still pre-1.0. There will often be breaking changes between versions. You'll also need to use the latest version of stable Rust and be on macOS or Linux. Add the following to your `Cargo.toml`:

```toml
gpui = { version = "*" }
```

 - [Ownership and data flow](src/_ownership_and_data_flow.rs)

Everything in GPUI starts with an `Application`. You can create one with `Application::new()`, and kick off your application by passing a callback to `Application::run()`. Inside this callback, you can create a new window with `App::open_window()`, and register your first root view. See [gpui.rs](https://www.gpui.rs/) for a complete example.

### Dependencies

GPUI has various system dependencies that it needs in order to work.

#### macOS

On macOS, GPUI uses Metal for rendering. In order to use Metal, you need to do the following:

- Install [Xcode](https://apps.apple.com/us/app/xcode/id497799835?mt=12) from the macOS App Store, or from the [Apple Developer](https://developer.apple.com/download/all/) website. Note this requires a developer account.

> Ensure you launch Xcode after installing, and install the macOS components, which is the default option.

- Install [Xcode command line tools](https://developer.apple.com/xcode/resources/)

  ```sh
  xcode-select --install
  ```

- Ensure that the Xcode command line tools are using your newly installed copy of Xcode:

  ```sh
  sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer
  ```

## The Big Picture

GPUI offers three different [registers](<https://en.wikipedia.org/wiki/Register_(sociolinguistics)>) depending on your needs:

- State management and communication with `Entity`'s. Whenever you need to store application state that communicates between different parts of your application, you'll want to use GPUI's entities. Entities are owned by GPUI and are only accessible through an owned smart pointer similar to an `Rc`. See the `app::context` module for more information.

- High level, declarative UI with views. All UI in GPUI starts with a view. A view is simply an `Entity` that can be rendered, by implementing the `Render` trait. At the start of each frame, GPUI will call this render method on the root view of a given window. Views build a tree of `elements`, lay them out and style them with a tailwind-style API, and then give them to GPUI to turn into pixels. See the `div` element for an all purpose swiss-army knife of rendering.

- Low level, imperative UI with Elements. Elements are the building blocks of UI in GPUI, and they provide a nice wrapper around an imperative API that provides as much flexibility and control as you need. Elements have total control over how they and their child elements are rendered and can be used for making efficient views into large lists, implement custom layouting for a code editor, and anything else you can think of. See the `element` module for more information.

Each of these registers has one or more corresponding contexts that can be accessed from all GPUI services. This context is your main interface to GPUI, and is used extensively throughout the framework.

## Other Resources

In addition to the systems above, GPUI provides a range of smaller services that are useful for building complex applications:

### Scroll-handle identity

`ScrollHandle::ptr_eq` reports whether two handles share the same retained scroll state. Clones of
one handle compare equal, while separately constructed handles remain distinct even when their
visible state is equal.

```rust
use gpui::ScrollHandle;

let handle = ScrollHandle::new();
let clone = handle.clone();
let distinct = ScrollHandle::new();

assert!(handle.ptr_eq(&clone));
assert!(!handle.ptr_eq(&distinct));
```

### Bounded streaming text layout

Very large logical lines can be laid out as ordered canonical text segments and source-zero-width
objects without assembling one complete source string. Every position combines a UTF-8 byte offset
with an exact adjacent-object gap. The consumer guarantees object identity uniqueness within each
source revision; GPUI checks local position, adjacency, order, and continuation consistency. A
session is borrowed from a window's text system, so it cannot outlive the window-affine shaping
context. Each text segment is an independent shaping context; only fixed-size exact placement
continuation crosses boundaries.

```no_run
use gpui::{
    SharedString, StreamingEndOfSource, StreamingInlineObject, StreamingLayoutBinding,
    StreamingLayoutLimits, StreamingLayoutPosition, StreamingLineFinalization,
    StreamingObjectGap, StreamingObjectId, StreamingObjectOrder, StreamingTextSegment, TextRun,
    Window, black, font, px,
};

fn shape_bounded_line(window: &Window) {
    let object_id = StreamingObjectId(41);
    let object_order = StreamingObjectOrder(7);
    let object_leading = StreamingLayoutPosition::with_gap(
        5,
        StreamingObjectGap::before(object_id, object_order),
    );
    let object_trailing = StreamingLayoutPosition::with_gap(
        5,
        StreamingObjectGap::after(object_id, object_order),
    );
    let binding = StreamingLayoutBinding {
        input_id: 1,
        segment_policy_id: 1,
        start_position: StreamingLayoutPosition::at(0),
        wrap_width: px(480.),
        font_size: px(14.),
        line_height: px(20.),
        limits: StreamingLayoutLimits {
            segment_bytes: 4096,
            runs: 64,
            decorations: 64,
            glyphs: 4096,
            wraps: 256,
            maps: 4097,
            fragments: 1,
            retained_items: 16_384,
            retained_bytes: 512 * 1024,
        },
    };
    let mut session = window.text_system().streaming_layout_session(binding).unwrap();
    let text = session.admit_text(StreamingTextSegment {
        input_id: 1,
        segment_policy_id: 1,
        ordinal: 0,
        logical_range: StreamingLayoutPosition::at(0)..object_leading,
        text: SharedString::new_static("hello"),
        runs: vec![TextRun {
            len: 5,
            font: font(".SystemUIFont"),
            color: black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
    }).unwrap();
    assert_eq!(text.item_charge.fragments, 1);

    let object = session.admit_inline_object(StreamingInlineObject {
        input_id: 1,
        segment_policy_id: 1,
        ordinal: 1,
        id: object_id,
        order: object_order,
        leading: object_leading,
        trailing: object_trailing,
        presentation: SharedString::new_static("•"),
        runs: vec![TextRun {
            len: "•".len(),
            font: font(".SystemUIFont"),
            color: black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        width: px(14.),
        height: px(20.),
        baseline: px(14.),
        background: None,
    }).unwrap();
    assert!(object.item_charge.object_ids > 0);
    assert_eq!(object.item_charge.object_ids, object.item_charge.object_orders);

    session.finalize_logical_line(StreamingLineFinalization {
        input_id: 1,
        segment_policy_id: 1,
        ordinal: 2,
        delimiter_range: None,
        next_position: object_trailing,
    }).unwrap();
    session.end_source(StreamingEndOfSource {
        input_id: 1,
        segment_policy_id: 1,
        ordinal: 3,
        source_extent: 5,
        position: object_trailing,
    }).unwrap();
}
```

Every successful admission reports both its existing exact retained-byte `charge` and a separate
exact semantic-record `item_charge`. The item breakdown includes retained text payloads, caller
style runs, private shaped runs, glyphs, decorations, wrap facts, maps, fragments, and compact
continuations, plus composite positions, gap witnesses, and object identity/order facts.
`StreamingLayoutSession::retained_item_charge` reports only its live continuation; cancelled
sessions report zero items. Logical-line finalization and end-of-source are explicit ordered inputs.

- Actions are user-defined structs that are used for converting keystrokes into logical operations in your UI. Use this for implementing keyboard shortcuts, such as cmd-q. See the `action` module for more information.

- Platform services, such as `quit the app` or `open a URL` are available as methods on the `app::App`.

- An async executor that is integrated with the platform's event loop. See the `executor` module for more information.,

- The `[gpui::test]` macro provides a convenient way to write tests for your GPUI applications. Tests also have their own kind of context, a `TestAppContext` which provides ways of simulating common platform input. See `app::test_context` and `test` modules for more details.

Currently, the best way to learn about these APIs is to read the Zed source code, ask us about it at a fireside hack, or drop a question in the [Zed Discord](https://zed.dev/community-links). We're working on improving the documentation, creating more examples, and will be publishing more guides to GPUI on our [blog](https://zed.dev/blog).
