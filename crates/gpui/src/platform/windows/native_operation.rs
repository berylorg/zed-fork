#![allow(missing_docs)]

use std::{
    cell::Cell,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

use anyhow::{Context as _, Result, ensure};
use futures::channel::oneshot;
use util::ResultExt;
#[cfg(feature = "test-support")]
use windows::Win32::Foundation::HWND;
use windows::Win32::{
    System::Ole::RevokeDragDrop,
    UI::WindowsAndMessaging::{DestroyWindow, IsWindowVisible},
};

use super::WindowsWindowInner;

pub struct WindowsHiddenWindowLease {
    handle: usize,
    _release: oneshot::Sender<()>,
    _not_sync: PhantomData<Cell<()>>,
}

impl WindowsHiddenWindowLease {
    pub fn raw_handle(&self) -> usize {
        self.handle
    }
}

pub struct WindowsHiddenWindowLeaseReleased {
    receiver: oneshot::Receiver<Result<WindowsHiddenWindowLeaseRelease>>,
}

impl Future for WindowsHiddenWindowLeaseReleased {
    type Output = Result<WindowsHiddenWindowLeaseRelease>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.receiver).poll(cx).map(|result| {
            result.context("native operation GUI completion ended without acknowledgement")?
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowsHiddenWindowLeaseRelease {
    pub close_requested: bool,
    pub native_destroyed: bool,
}

#[derive(Default)]
pub(super) struct NativeOperationState {
    issued: bool,
    active: bool,
    published: bool,
    close_requested: bool,
    destroy_requested: bool,
    destroy_scheduled: bool,
    native_destroyed: bool,
}

impl WindowsWindowInner {
    pub(super) fn lease_hidden_native_window(
        self: &Rc<Self>,
    ) -> Result<(WindowsHiddenWindowLease, WindowsHiddenWindowLeaseReleased)> {
        {
            let mut state = self.native_operation.borrow_mut();
            ensure!(
                !state.issued,
                "a native operation has already been issued for this window"
            );
            ensure!(
                !state.published
                    && !state.close_requested
                    && !state.destroy_requested
                    && !state.native_destroyed,
                "native window is published, closing or destroyed"
            );
            ensure!(
                !unsafe { IsWindowVisible(self.hwnd) }.as_bool(),
                "native operation requires a hidden window"
            );
            state.issued = true;
            state.active = true;
        }
        let (release, released) = oneshot::channel();
        let (complete, completion) = oneshot::channel();
        let this = self.clone();
        self.executor
            .spawn(async move {
                let _ = released.await;
                let destroy_requested = {
                    let mut state = this.native_operation.borrow_mut();
                    state.active = false;
                    state.destroy_requested
                };
                let result = if destroy_requested {
                    this.destroy_native_window()
                } else {
                    Ok(())
                }
                .map(|()| {
                    let state = this.native_operation.borrow();
                    WindowsHiddenWindowLeaseRelease {
                        close_requested: state.close_requested,
                        native_destroyed: state.native_destroyed,
                    }
                });
                let _ = complete.send(result);
            })
            .detach();
        Ok((
            WindowsHiddenWindowLease {
                handle: self.hwnd.0 as usize,
                _release: release,
                _not_sync: PhantomData,
            },
            WindowsHiddenWindowLeaseReleased {
                receiver: completion,
            },
        ))
    }

    pub(super) fn native_close_requested(&self) -> bool {
        self.native_operation.borrow().close_requested
    }

    pub(super) fn native_exposure_blocked(&self) -> bool {
        let state = self.native_operation.borrow();
        state.active || state.close_requested || state.destroy_requested || state.native_destroyed
    }

    pub(super) fn latch_native_close_request(&self) -> bool {
        let mut state = self.native_operation.borrow_mut();
        if state.issued && !state.published {
            state.close_requested = true;
            true
        } else {
            false
        }
    }

    pub(super) fn native_did_publish(&self) {
        self.native_operation.borrow_mut().published = true;
    }

    pub(super) fn native_did_destroy(&self) {
        self.native_operation.borrow_mut().native_destroyed = true;
    }

    pub(super) fn request_native_destruction(self: &Rc<Self>) {
        {
            let mut state = self.native_operation.borrow_mut();
            state.destroy_requested = true;
            if state.active || state.native_destroyed || state.destroy_scheduled {
                return;
            }
            state.destroy_scheduled = true;
        }
        let this = self.clone();
        self.executor
            .spawn(async move {
                this.destroy_native_window().log_err();
            })
            .detach();
    }

    fn destroy_native_window(&self) -> Result<()> {
        {
            let mut state = self.native_operation.borrow_mut();
            if state.native_destroyed {
                return Ok(());
            }
            ensure!(
                !state.active,
                "native destruction cannot run during a worker operation"
            );
            state.destroy_scheduled = true;
        }
        unsafe {
            RevokeDragDrop(self.hwnd).log_err();
        }
        #[cfg(feature = "test-support")]
        observe_native_destruction(self.hwnd);
        unsafe { DestroyWindow(self.hwnd) }.context("destroying the exact owned native window")
    }
}

#[cfg(feature = "test-support")]
type DestructionObserver = Box<dyn FnMut(usize)>;

#[cfg(feature = "test-support")]
thread_local! {
    static DESTRUCTION_OBSERVER: std::cell::RefCell<Option<DestructionObserver>> = std::cell::RefCell::new(None);
}

#[cfg(feature = "test-support")]
pub fn with_windows_window_destruction_observer_for_test<R>(
    observer: impl FnMut(usize) + 'static,
    operation: impl FnOnce() -> R,
) -> R {
    struct RestoreObserver(Option<DestructionObserver>);
    impl Drop for RestoreObserver {
        fn drop(&mut self) {
            DESTRUCTION_OBSERVER.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }
    let _restore =
        RestoreObserver(DESTRUCTION_OBSERVER.with(|slot| slot.replace(Some(Box::new(observer)))));
    operation()
}

#[cfg(feature = "test-support")]
pub(super) fn observe_native_destruction(hwnd: HWND) {
    DESTRUCTION_OBSERVER.with(|slot| {
        if let Some(observer) = slot.borrow_mut().as_mut() {
            observer(hwnd.0 as usize);
        }
    });
}
