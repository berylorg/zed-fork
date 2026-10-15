# Checked Window Minimum Qualification

This evidence qualifies the [Windows minimum-update boundary](../design.md#checked-window-minimum-updates-on-windows),
its read-only minimized-state observation, initial minimum enforcement and test-platform support.
It does not accept Beryl's sizing, appearance publication or restoration consumers.

## Inputs

Qualification used base `14d9622001ecbfa0db4a5530b989b649690472a1` plus final source/authority patch
SHA256 `FC7A87CC4D411DD407074AAA74E4E6D78C66CABB44158DC041F5281821DE8226`.
The unchanged lock SHA256 is `CB57266DEF1A468DCF261980B46EC617640C8B2A69AE522C631122D795339C7E`.
This evidence file is outside that patch. No manifests or dependencies changed.

The observed native host was Windows 11 Pro build 26200. Commands used stable rustc 1.97.1,
LLVM linking, one Cargo job, nonincremental compilation, graph-wide debug information disabled
and serial tests. Production features were `--no-default-features --features windows-manifest`;
test features additionally enabled `test-support`. Existing native fixture deadlines remained
unchanged. Process error mode was restored after each native packet.

## Results

Locked metadata passed. The final production implementation passed the normal library check in
6.70 seconds. Nextest `9c151b18-d7e2-4831-9065-fb43e9b98d02` passed all 29 cases across
`window_minimum`, `window_minimum_native`, `window_publication`,
`recoverable_window_destruction` and `application_lifetime` in 10.407 seconds.

A subsequent native-test-only precision correction passed all seven native cases in
`e8472023-826a-4241-a637-fbe3aeb32a35` (4.234 seconds). Production, dependency, configuration,
model and the other 18 existing lifecycle cases were unchanged; the previous production check
and 22 nonchanged cases remain qualified. Raw inputs, patches and output are retained under
Beryl's `.aipm/tasks/native-main-window-minimum/run-003` and `run-004` through integration review.

Native cases exercise original HWND/client bounds and tracking, unchanged origin/focus/visibility,
hidden first-publication authority, minimized refusal and actual restoration, maximized and
fullscreen outcomes, pre-exposure construction, native refusal, failed verification after genuine
growth, receiver disposal and deferred destruction. Exact-fit and one-physical-pixel-over normal
restore bounds are checked for custom and native frames, including hidden maximized construction.

## Review And Limits

Independent semantic and lifecycle review passes on the final patch. It verifies retained original
ownership, shared native exclusion, receiver-independent settlement, non-success constraint
restoration and closure without consuming hidden-publication authority.

Review corrected a conservative maximized refusal: normal restore dimensions must use normal
nonclient insets rather than the current maximized border. Test calculations also must promote
the same rounded `f32` DPI scale as production, rather than recompute that ratio directly in
`f64`. The native helper retains adjacent-representable-value and exact-ceiling checks.

This is observed Windows qualification, without a Windows 10, custom-DPI hardware or monitor-
transition matrix claim. Applied acknowledges tracking and current geometry, not compositor
paint or immunity to later arbitrary native geometry changes. Unsupported backends retain the
explicit unavailable boundary. Beryl's original-window metrics, appearance transaction, hidden
registration and final integrated consumers require their own acceptance evidence.
