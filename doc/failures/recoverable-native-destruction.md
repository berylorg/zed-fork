# Recoverable Native Destruction

## Invalidated Lifetime Assumption

Beryl's nonfinal close can report native destruction failure after durable removal. The former
GPUI removal path deleted the logical window and dropped its platform wrapper before asynchronous
native destruction completed, so observing a failure could not preserve the same usable window.
The owned recoverable attempt instead retains logical identity until terminal native settlement;
settled refusal preserves the same root/window and permits a separately activated fresh attempt.

Retaining the wrapper also exposed stale native-handle paths outside drawing: title/appearance,
deferred resize/prompt, raw-handle access and drag/drop. These paths now respect native lifetime.
The platform's background redraw/GPU registry needed the same correction. Removing a handle under
its write lock while a GPU sender held the read lock across synchronous native dispatch could
deadlock. Physical handle retirement now precedes reuse, foreground dispatch retains exact owner
snapshots and rechecks native lifetime, and no registry lock spans native callbacks. A separate
bounded unsettled count preserves logical cleanup custody and default exit exclusion. Forced
redraw does not clear the coalescing token for an already queued ordinary redraw.

## Verification

Independent lifecycle review covered the complete production change, eight focused cases and
their helper, App slot removal, native callbacks, confirmation, dispatch and default exit. No
blocking findings remain. Corrected native run `310f7f49-01dd-4c56-847c-69e1151b2e5a` passed all
12 cases in 6.457 seconds: eight recoverable-destruction cases and four existing application
lifetime cases. Evidence is retained under Beryl's `.tmp/ordinary-native-evidence` as
`recoverable-gpui-nextest-accepted.txt`. The native WinRT worker stack was unchanged; only the Rust
test harness used its documented 32 MiB stack. Nextest imposed a 60-second per-case deadline.

The native run used the tracked fork lockfile: nextest's nested Cargo did not forward the outer
local lock-path option. Direct local production checking uses the ignored local lock instead.
Both direct local and canonical shipped-feature library checks passed, without `test-support`;
canonical locked metadata passed. All 11 production/test inputs matched the reviewed SHA-256
inventory. Tracked manifests and lockfile were unchanged. Only the existing Taffy float-literal
warnings and dependency future-compatibility notice remained. This accepts the fork boundary;
downstream canonical pin qualification belongs to Beryl. The boundary does not itself restore
durable membership, claims or editor bindings in Beryl.
