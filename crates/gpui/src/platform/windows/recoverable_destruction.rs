#![allow(missing_docs)]

#[cfg(feature = "test-support")]
use std::rc::Weak;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

use anyhow::{Context as _, Result, ensure};
use futures::channel::oneshot;
use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, IsWindow, IsWindowVisible};

use super::{WindowsWindowInner, window::window_from_hwnd};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WindowsNativeWindowDestructionOutcome {
    Destroyed,
    Survived { error: String },
    Unresolved { error: String },
}

pub struct WindowsNativeWindowDestruction {
    state: Rc<DestructionState>,
}

impl WindowsNativeWindowDestruction {
    #[cfg(feature = "test-support")]
    pub fn refresh_native_windows_for_test(&self) {
        if let Some(owner) = self.state.owner.upgrade() {
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    owner.platform_window_handle,
                    super::events::WM_GPUI_REDRAW_NATIVE_WINDOWS,
                    Some(windows::Win32::Foundation::WPARAM(owner.validation_number)),
                    Some(windows::Win32::Foundation::LPARAM(1)),
                )
            };
        }
    }
    #[cfg(feature = "test-support")]
    pub fn replay_completion_for_test(&self, outcome: WindowsNativeWindowDestructionOutcome) {
        if let Some(owner) = self.state.owner.upgrade() {
            owner.finish_recoverable_destruction(&self.state, outcome);
        }
    }

    pub fn outcome(&self) -> Option<WindowsNativeWindowDestructionOutcome> {
        self.state.outcome.borrow().clone()
    }

    pub fn same_attempt(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.state, &other.state)
    }
}

pub struct WindowsNativeWindowDestructionCompleted {
    receiver: oneshot::Receiver<WindowsNativeWindowDestructionOutcome>,
}

impl Future for WindowsNativeWindowDestructionCompleted {
    type Output = Result<WindowsNativeWindowDestructionOutcome>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.receiver)
            .poll(cx)
            .map(|result| result.context("recoverable native destruction completion was lost"))
    }
}

pub(super) struct DestructionState {
    #[cfg(feature = "test-support")]
    owner: Weak<WindowsWindowInner>,
    scheduled: Cell<bool>,
    outcome: RefCell<Option<WindowsNativeWindowDestructionOutcome>>,
    completion: RefCell<Option<oneshot::Sender<WindowsNativeWindowDestructionOutcome>>>,
    #[cfg(feature = "test-support")]
    fault: Option<WindowsNativeWindowDestructionTestFault>,
}

impl WindowsWindowInner {
    pub(super) fn recoverable_destruction_blocks_removal(&self) -> bool {
        self.recoverable_destruction
            .borrow()
            .as_ref()
            .is_some_and(|attempt| {
                !matches!(
                    attempt.outcome.borrow().as_ref(),
                    Some(WindowsNativeWindowDestructionOutcome::Destroyed)
                )
            })
    }

    pub(super) fn begin_recoverable_destruction(
        self: &Rc<Self>,
    ) -> Result<(
        WindowsNativeWindowDestruction,
        WindowsNativeWindowDestructionCompleted,
    )> {
        ensure!(
            self.recoverable_destruction.borrow().is_none(),
            "native window already has a destruction attempt"
        );
        {
            let state = self.native_operation.borrow();
            ensure!(
                state.published
                    && !state.active
                    && !state.destroy_requested
                    && !state.native_destroyed,
                "recoverable destruction requires an available published native window"
            );
        }
        ensure!(
            unsafe { IsWindowVisible(self.hwnd) }.as_bool(),
            "recoverable destruction requires a visible native window"
        );
        let (sender, receiver) = oneshot::channel();
        let attempt = Rc::new(DestructionState {
            #[cfg(feature = "test-support")]
            owner: Rc::downgrade(self),
            scheduled: Cell::new(false),
            outcome: RefCell::new(None),
            completion: RefCell::new(Some(sender)),
            #[cfg(feature = "test-support")]
            fault: DESTRUCTION_FAULT.with(Cell::get),
        });
        *self.recoverable_destruction.borrow_mut() = Some(attempt.clone());
        self.native_operation.borrow_mut().destroy_requested = true;
        let confirmation = self.confirmation.borrow().clone();
        if let Some(confirmation) = confirmation {
            if confirmation.cleanup_unresolved() {
                self.recoverable_confirmation_unresolved();
            } else if let Err(error) = confirmation.cancel() {
                self.finish_recoverable_destruction(
                    &attempt,
                    WindowsNativeWindowDestructionOutcome::Unresolved {
                        error: format!("{error:#}"),
                    },
                );
            }
        } else {
            self.dispatch_recoverable_destruction();
        }
        Ok((
            WindowsNativeWindowDestruction { state: attempt },
            WindowsNativeWindowDestructionCompleted { receiver },
        ))
    }

    pub(super) fn recoverable_confirmation_unresolved(&self) {
        let attempt = self.recoverable_destruction.borrow().clone();
        if let Some(attempt) = attempt {
            self.finish_recoverable_destruction(
                &attempt,
                WindowsNativeWindowDestructionOutcome::Unresolved {
                    error: "native confirmation cleanup did not settle".into(),
                },
            );
        }
    }

    pub(super) fn dispatch_recoverable_destruction(self: &Rc<Self>) {
        let attempt = self.recoverable_destruction.borrow().clone();
        let Some(attempt) = attempt else { return };
        if attempt.outcome.borrow().is_some()
            || self.confirmation.borrow().is_some()
            || attempt.scheduled.replace(true)
        {
            return;
        }
        let owner = self.clone();
        self.executor
            .spawn(async move {
                if !owner.owns_destruction_attempt(&attempt) || attempt.outcome.borrow().is_some() {
                    return;
                }
                let result = owner.run_recoverable_destruction(&attempt);
                owner.finish_recoverable_destruction(&attempt, result);
            })
            .detach();
    }

    fn owns_destruction_attempt(&self, attempt: &Rc<DestructionState>) -> bool {
        self.recoverable_destruction
            .borrow()
            .as_ref()
            .is_some_and(|current| Rc::ptr_eq(current, attempt))
    }

    fn run_recoverable_destruction(
        self: &Rc<Self>,
        attempt: &DestructionState,
    ) -> WindowsNativeWindowDestructionOutcome {
        let already_finished = self.native_operation.borrow().native_destroy_finished;
        if already_finished {
            return WindowsNativeWindowDestructionOutcome::Destroyed;
        }
        #[cfg(feature = "test-support")]
        super::native_operation::observe_native_destruction(self.hwnd);
        #[cfg(feature = "test-support")]
        let result = if attempt.fault == Some(WindowsNativeWindowDestructionTestFault::Refuse) {
            Err(anyhow::anyhow!("injected DestroyWindow refusal"))
        } else {
            unsafe { DestroyWindow(self.hwnd) }.context("destroying the exact owned native window")
        };
        #[cfg(not(feature = "test-support"))]
        let result = {
            let _ = attempt;
            unsafe { DestroyWindow(self.hwnd) }.context("destroying the exact owned native window")
        };
        let (destroyed, finished) = {
            let state = self.native_operation.borrow();
            (state.native_destroyed, state.native_destroy_finished)
        };
        #[cfg(feature = "test-support")]
        let finished = finished
            && attempt.fault != Some(WindowsNativeWindowDestructionTestFault::MissingSettlement);
        if finished {
            return WindowsNativeWindowDestructionOutcome::Destroyed;
        }
        let error = match result {
            Ok(()) => "native destruction returned without final destruction evidence".into(),
            Err(error) => format!("{error:#}"),
        };
        if !destroyed
            && unsafe { IsWindow(Some(self.hwnd)) }.as_bool()
            && window_from_hwnd(self.hwnd).is_some_and(|owner| Rc::ptr_eq(&owner, self))
        {
            WindowsNativeWindowDestructionOutcome::Survived { error }
        } else {
            WindowsNativeWindowDestructionOutcome::Unresolved { error }
        }
    }

    fn finish_recoverable_destruction(
        &self,
        attempt: &Rc<DestructionState>,
        outcome: WindowsNativeWindowDestructionOutcome,
    ) {
        if !self.owns_destruction_attempt(attempt) || attempt.outcome.borrow().is_some() {
            return;
        }
        *attempt.outcome.borrow_mut() = Some(outcome.clone());
        match &outcome {
            WindowsNativeWindowDestructionOutcome::Destroyed => {
                self.notify_native_window_closed(self.hwnd)
            }
            WindowsNativeWindowDestructionOutcome::Survived { .. } => {
                self.recoverable_destruction.borrow_mut().take();
                let mut state = self.native_operation.borrow_mut();
                state.destroy_requested = false;
                state.destroy_scheduled = false;
            }
            WindowsNativeWindowDestructionOutcome::Unresolved { .. } => {}
        }
        let sender = attempt.completion.borrow_mut().take();
        if let Some(sender) = sender {
            let _ = sender.send(outcome);
        }
    }
}

#[cfg(feature = "test-support")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsNativeWindowDestructionTestFault {
    Refuse,
    MissingSettlement,
}

#[cfg(feature = "test-support")]
thread_local! {
    static DESTRUCTION_FAULT: Cell<Option<WindowsNativeWindowDestructionTestFault>> = const { Cell::new(None) };
}

#[cfg(feature = "test-support")]
pub fn with_windows_window_destruction_fault_for_test<R>(
    fault: WindowsNativeWindowDestructionTestFault,
    operation: impl FnOnce() -> R,
) -> R {
    struct RestoreFault(Option<WindowsNativeWindowDestructionTestFault>);
    impl Drop for RestoreFault {
        fn drop(&mut self) {
            DESTRUCTION_FAULT.with(|slot| slot.set(self.0));
        }
    }
    let _restore = RestoreFault(DESTRUCTION_FAULT.with(|slot| slot.replace(Some(fault))));
    operation()
}
