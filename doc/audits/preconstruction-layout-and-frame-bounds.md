# Preconstruction Layout And Frame Bounds Qualification

This accepts two bounded consumers of existing GPUI state: checked line layout through the
shared text system before window construction, and an opt-in read-only rendered selector-bounds
query for native integration tests. Their contracts are in [the design](../design.md).

## Inputs And Results

Base is `f1bb630dd69e2da0ea5d5378ee82b60ed3525bce`; final source/design patch SHA256 is
`DEFA09D06B2AD3B78B9307CADC5D771F1CEE76EA4EC15E97DCE7526D80831B45`, excluding this audit.
The unchanged lock SHA256 is `CB57266DEF1A468DCF261980B46EC617640C8B2A69AE522C631122D795339C7E`.
No manifest or dependency changed. Windows 11 Pro build 26200 qualification uses stable Rust,
LLVM linking, one job, nonincremental compilation, no normal debug information and serial tests.

The normal production GPUI library check passes in 8.15 seconds. With `windows-manifest,test-support`,
all 13 `font_environment` cases pass in 1.294 seconds
(`071dc8de-6f56-4713-9b8d-ba8bf770fd0f`). The new regression obtains actual checked layout from
App's shared text system before adding a window, then verifies window checked-layout and renderer
width parity. It also preserves and checks the existing forced glyph-position semantics.
Raw input patches and results remain under Beryl's native-minimum task `run-006` and `run-007`
through the final integration consumers.

The initial new test incorrectly assumed `force_width` replaces total line width. Existing
behavior changes glyph positions only. Correcting the expectation changes no production code;
the initial packet passed the other 12 cases, and the final packet passes all 13.

## Review And Evidence Reuse

Independent source review accepts moving the existing checked-layout body unchanged from
`WindowTextSystem` into `TextSystem`; existing dereference preserves window access. Font resolution,
platform checked layout, environment revalidation and shared locking remain the same. No window,
per-window cache, native operation or new owner is introduced. The current-frame bounds query
only copies one existing map entry and is absent from normal production builds.

The [minimum-update qualification](window-minimum.md) remains valid for its unchanged native
implementation and 29 model/native/lifecycle cases. This audit does not accept Beryl's application
integration. Its genuine native composition test must consume the bounds query separately.
