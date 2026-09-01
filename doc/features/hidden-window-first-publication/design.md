# Goals

Allow GPUI consumers to construct and coherently prepare a native window without exposing an
incomplete surface, then explicitly publish that same window as its first user-visible surface.

## Non-goals

- Defining Beryl startup, restoration, thread acquisition, abandonment, capacity, or shell
  composition.
- Providing a general repeated hide-and-show API for an already published window.
- Creating, substituting, or swapping in a replacement window at publication.
- Combining first publication with window activation or focus.
- Governing unrelated Zed or GPUI window lifecycle behavior.

# Decisions

## Hidden Construction

- A native window deliberately created hidden remains absent from user-visible and interactive
  window presentation until explicit first publication.
- Drawing, root-view updates, title and bounds changes, callback installation, renderer
  preparation, and other ordinary preparation do not implicitly publish a hidden window.
- Hidden construction retains the same GPUI window identity, root entity, native window, renderer
  ownership, callbacks, and configured properties that first publication later exposes.
- Construction options fix the initially windowed, maximized, or fullscreen native state before
  the hidden window is created. First publication preserves that state.
- Maximize and fullscreen requests made while the window remains unpublished are inert: they do
  not change its fixed initial state, publish it, or activate it. Those operations resume their
  ordinary behavior after publication.

## First Publication

- First publication makes the same hidden native window eligible to become visible. It never
  creates, substitutes, or swaps in another native or GPUI window.
- The first visible surface uses the latest coherently prepared ordinary root state without an
  intermediate blank, placeholder, or construction surface introduced by GPUI.
- Success means GPUI accepted and performed the applicable platform visibility transition. It is
  not an acknowledgement that an OS compositor painted or a user observed a frame.
- Windows created for immediate display retain their existing behavior.

## Focus And Repetition

- First publication does not activate or focus the window; consumers use GPUI's separate
  activation boundary when desired.
- Publication may perform only the platform-required initial insertion into visible stacking or
  tab order. It performs no additional raise, front-ordering, or reorder operation.
- Repeating first publication after success is a non-reactivating no-op. It does not repeat the
  platform transition, recreate the window, or change ordering.

## Closure And Failure

- Closing a hidden window before first publication leaves it never published.
- Attempting publication through a window handle that no longer resolves fails through the
  ordinary handle-update boundary and creates no replacement.
- A platform publication failure reports failure, leaves the window unpublished and eligible for
  an explicit retry or close, and exposes no substitute surface.

## Supported Operating Envelope

- The contract applies consistently to GPUI native backends that accept initially hidden windows.
- Windowed hidden construction is supported on every such backend. Initial maximized or fullscreen
  state is supported only where the backend can establish it without exposing the window.
- On macOS, hidden fullscreen construction fails before native exposure. Hidden windowed and
  maximized construction and ordinary immediate fullscreen behavior remain supported.
- GPUI's test platform exposes bounded content-free state sufficient to verify hidden creation,
  identity-preserving first publication, repetition, ordinary immediate display, failure, and
  close-before-publication, plus fixed initial-state behavior while unpublished.
- Headless platforms that reject native-window creation do not fabricate a publication lifecycle.

# Engineering Rigor

Profile: `production-application/v1`

Modifiers: none
