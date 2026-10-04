# Scope

Implement the checked Windows clipboard boundary in [design.md](design.md#checked-windows-clipboard-boundary)
for Beryl's root plan Phase 728. Operator approved native complete acknowledgement and reads bounded
before allocation. This fork plan owns the dependency implementation; Beryl owns canonical dependency
publication and composer integration. Preserve unrelated work and perform independent semantic review.

# Phase 1: Implement Checked Windows Clipboard Boundary (wip)

Expose typed caller-bounded clipboard acquisition and acknowledged text/metadata or encoded-image
writing. Windows checks complete native allocation/encoding lengths before output allocation and
reports ownership, format, size, malformed, write/close and unsupported outcomes. Preserve exact
snapshot sequence and release native handles on every path. Other backends return typed unavailable
for this boundary. Qualify exact fit/one-over, malformed input, text and metadata failure, read and
close failure, and native Windows ownership/format publication through an isolated harness without
launching Beryl. Complete independent semantic review before publication. Do not alter unrelated
legacy clipboard convenience consumers or introduce retries/readback acknowledgement.

Resolved on 2026-10-04: native allocation size is not an exact encoded image length. Independent
review rejected the writer's exact-size assumption. Beryl's
[representation proposal](../../beryl/doc/failures/checked-clipboard-image-representation.md)
was approved by the Operator and is now defined in the fork design. Implement the bounded private
companion and qualify padded/nonaligned payloads, consistency, bounds and partial publication.
Preserve source
as unaccepted work and keep dependency publication pending. The independently accepted isolated
harness uses a private station clipboard. Complete native qualification before
acceptance; canonical integration remains Beryl-owned.

Current milestone: the shared harness has been replaced with a create-only private window
station/desktop, verifying exact original bindings and owned-handle cleanup. No further
qualification may acquire or modify the Operator's
actively used clipboard. The private harness passed independent review, locked metadata, focused
Cargo check and all 20 current deterministic cases; analyzer restart succeeded. Native acceptance and
source publication remain pending an Administrator-terminal run, since named station creation is
denied in the unelevated agent process. See Beryl's
[qualification evidence](../../beryl/doc/audits/composer-marker-feedback/checked-native-clipboard.md).
