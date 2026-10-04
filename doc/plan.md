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

Blocked on 2026-10-04: native allocation size is not an exact encoded image length. Independent
review rejected the writer's exact-size assumption. Beryl's
[representation proposal](../../beryl/doc/failures/checked-clipboard-image-representation.md)
requires Operator resolution before production changes or native qualification. Preserve source
as unaccepted work and keep dependency publication pending. The isolated harness correction
checks ownership before every mutation; independent review accepted the correction and all 16
focused deterministic cases passed, including three preservation controls. Native qualification
remains unexecuted; the image blocker prevents phase acceptance.
