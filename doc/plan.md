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
harness correction checks ownership before every mutation. Complete native qualification before
acceptance; canonical integration remains Beryl-owned.

Current milestone: corrected source passed independent review and all 23 deterministic cases.
Native preparation failed at `OpenClipboard` with Win32 access denied (error 5) before mutation;
a separate session probe also denied access. Native acceptance and source publication remain
blocked pending a Windows execution session with clipboard access. See Beryl's
[qualification evidence](../../beryl/doc/audits/composer-marker-feedback/checked-native-clipboard.md).
