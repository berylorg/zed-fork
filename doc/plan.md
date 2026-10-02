# Scope

Implement the Beryl-owned recoverable native window destruction boundary under
[fork authority](design.md#recoverable-native-window-destruction-on-windows). Beryl's approved
healthy nonfinal-close restoration requires preservation of the exact GPUI/native window after
settled destruction failure. Keep unrelated Zed behavior outside this work.

# Phase 1: Retain Windows Through Recoverable Native Destruction (wip)

Add the bounded per-window attempt and exact settlement boundary without early GPUI slot removal.
Preserve existing irreversible removal, confirmation cleanup and explicit process lifetime.
Verify real Windows successful destruction, injected refusal with unchanged window/root identity,
fresh later success, duplicate/stale callbacks, receiver abandonment and confirmation ordering.
Run focused GPUI checks and independent lifecycle review. Consumer storage/editor restoration
belongs to Beryl's separate ordinary-close composition; toolkit-only checks cannot accept it.
