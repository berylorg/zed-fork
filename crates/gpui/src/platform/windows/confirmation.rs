#![allow(missing_docs)]

use super::WindowsWindowInner;
use anyhow::{Context as _, Result, ensure};
use futures::channel::oneshot;
use std::{
    cell::Cell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        UI::{
            Controls::{
                TASKDIALOG_BUTTON, TASKDIALOG_NOTIFICATIONS, TASKDIALOGCONFIG,
                TDF_ALLOW_DIALOG_CANCELLATION, TDF_CALLBACK_TIMER, TDF_POSITION_RELATIVE_TO_WINDOW,
                TDM_CLICK_BUTTON, TDN_CREATED, TDN_DESTROYED, TDN_TIMER, TaskDialogIndirect,
            },
            WindowsAndMessaging::{
                GetForegroundWindow, IDCANCEL, IDOK, PostMessageW, SetForegroundWindow,
            },
        },
    },
    core::{HRESULT, PCWSTR},
};

pub struct WindowsNativeConfirmationRequest {
    title: String,
    message: String,
    cancel: String,
    confirm: String,
    #[cfg(feature = "test-support")]
    fault: Option<WindowsNativeConfirmationTestFault>,
}

impl WindowsNativeConfirmationRequest {
    pub fn new(title: &str, message: &str, cancel: &str, confirm: &str) -> Result<Self> {
        for (value, limit) in [(title, 256), (message, 4096), (cancel, 128), (confirm, 128)] {
            ensure!(
                !value.is_empty() && value.len() <= limit && !value.contains('\0'),
                "invalid native confirmation text"
            );
        }
        Ok(Self {
            title: title.into(),
            message: message.into(),
            cancel: cancel.into(),
            confirm: confirm.into(),
            #[cfg(feature = "test-support")]
            fault: None,
        })
    }

    #[cfg(feature = "test-support")]
    pub fn with_fault_for_test(mut self, fault: WindowsNativeConfirmationTestFault) -> Self {
        self.fault = Some(fault);
        self
    }
}

#[cfg(feature = "test-support")]
#[derive(Clone, Copy, PartialEq)]
pub enum WindowsNativeConfirmationTestFault {
    NativeOpen,
    MissingDestruction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsNativeConfirmationOutcome {
    Confirmed,
    Cancelled,
}

#[derive(Default)]
pub(super) struct ConfirmationState {
    hwnd: Cell<Option<HWND>>,
    cancelled: Cell<bool>,
    reveal: Cell<bool>,
    created: Cell<bool>,
    destroyed: Cell<bool>,
    settled: Cell<bool>,
    cleanup_settled: Cell<bool>,
}

impl ConfirmationState {
    pub(super) fn cleanup_unresolved(&self) -> bool {
        self.settled.get() && !self.cleanup_settled.get()
    }

    pub(super) fn cancel(&self) -> Result<()> {
        self.cancelled.set(true);
        if let Some(hwnd) = self.hwnd.get() {
            unsafe {
                PostMessageW(
                    Some(hwnd),
                    TDM_CLICK_BUTTON.0 as u32,
                    WPARAM(IDCANCEL.0 as usize),
                    LPARAM(0),
                )
            }
            .context("cancelling native confirmation")?;
        }
        Ok(())
    }
}

pub struct WindowsNativeConfirmation {
    state: Rc<ConfirmationState>,
}

impl WindowsNativeConfirmation {
    pub fn cleanup_settled(&self) -> bool {
        self.state.cleanup_settled.get()
    }

    pub fn reveal(&self) -> Result<()> {
        ensure!(
            !self.state.settled.get() && !self.state.cancelled.get(),
            "native confirmation is no longer revealable"
        );
        self.state.reveal.set(true);
        if let Some(hwnd) = self.state.hwnd.get() {
            ensure!(
                unsafe { GetForegroundWindow() } == hwnd
                    || unsafe { SetForegroundWindow(hwnd) }.as_bool(),
                "native confirmation activation was refused"
            );
        }
        Ok(())
    }

    pub fn cancel(&self) -> Result<()> {
        self.state.cancel()
    }
}

impl Drop for WindowsNativeConfirmation {
    fn drop(&mut self) {
        let _ = self.state.cancel();
    }
}

pub struct WindowsNativeConfirmationCompleted {
    receiver: oneshot::Receiver<Result<WindowsNativeConfirmationOutcome>>,
}

impl Future for WindowsNativeConfirmationCompleted {
    type Output = Result<WindowsNativeConfirmationOutcome>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.receiver)
            .poll(cx)
            .map(|result| result.context("native confirmation completion authority was lost")?)
    }
}

unsafe extern "system" fn callback(
    hwnd: HWND,
    notification: TASKDIALOG_NOTIFICATIONS,
    _: WPARAM,
    _: LPARAM,
    data: isize,
) -> HRESULT {
    let state = unsafe { &*(data as *const ConfirmationState) };
    if notification == TDN_CREATED {
        state.created.set(true);
        state.hwnd.set(Some(hwnd));
        if state.reveal.get() && !state.cancelled.get() {
            let _ = unsafe { SetForegroundWindow(hwnd) };
        }
        if state.cancelled.get() {
            let _ = state.cancel();
        }
    } else if notification == TDN_TIMER && state.cancelled.get() {
        let _ = state.cancel();
    } else if notification == TDN_DESTROYED {
        state.hwnd.set(None);
        state.destroyed.set(true);
    }
    HRESULT(0)
}

impl WindowsWindowInner {
    pub(super) fn begin_native_confirmation(
        self: &Rc<Self>,
        request: WindowsNativeConfirmationRequest,
    ) -> Result<(
        WindowsNativeConfirmation,
        WindowsNativeConfirmationCompleted,
    )> {
        self.admit_native_confirmation()?;
        let state = Rc::new(ConfirmationState::default());
        *self.confirmation.borrow_mut() = Some(state.clone());
        let (sender, receiver) = oneshot::channel();
        let owner = self.clone();
        let operation = state.clone();
        self.executor
            .spawn(async move {
                let result = if operation.cancelled.get() {
                    Ok(WindowsNativeConfirmationOutcome::Cancelled)
                } else {
                    run_dialog(owner.hwnd, &request, &operation)
                };
                operation.settled.set(true);
                if !operation.created.get() || operation.destroyed.get() {
                    owner.confirmation.borrow_mut().take();
                    operation.cleanup_settled.set(true);
                    owner.finish_native_confirmation();
                } else {
                    owner.recoverable_confirmation_unresolved();
                }
                let _ = sender.send(result);
            })
            .detach();
        Ok((
            WindowsNativeConfirmation { state },
            WindowsNativeConfirmationCompleted { receiver },
        ))
    }
}

fn run_dialog(
    owner: HWND,
    request: &WindowsNativeConfirmationRequest,
    state: &ConfirmationState,
) -> Result<WindowsNativeConfirmationOutcome> {
    use windows::Win32::UI::{
        Input::KeyboardAndMouse::IsWindowEnabled, WindowsAndMessaging::IsWindowVisible,
    };
    ensure!(
        unsafe { IsWindowVisible(owner) }.as_bool() && unsafe { IsWindowEnabled(owner) }.as_bool(),
        "native confirmation requires a visible, enabled owner"
    );
    #[cfg(feature = "test-support")]
    ensure!(
        request.fault != Some(WindowsNativeConfirmationTestFault::NativeOpen),
        "injected native confirmation creation failure"
    );
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    let title = wide(&request.title);
    let message = wide(&request.message);
    let cancel = wide(&request.cancel);
    let confirm = wide(&request.confirm);
    let buttons = [
        TASKDIALOG_BUTTON {
            nButtonID: IDCANCEL.0,
            pszButtonText: PCWSTR(cancel.as_ptr()),
        },
        TASKDIALOG_BUTTON {
            nButtonID: IDOK.0,
            pszButtonText: PCWSTR(confirm.as_ptr()),
        },
    ];
    let config = TASKDIALOGCONFIG {
        cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
        hwndParent: owner,
        dwFlags: TDF_ALLOW_DIALOG_CANCELLATION
            | TDF_CALLBACK_TIMER
            | TDF_POSITION_RELATIVE_TO_WINDOW,
        pszWindowTitle: PCWSTR(title.as_ptr()),
        pszMainInstruction: PCWSTR(message.as_ptr()),
        cButtons: 2,
        pButtons: buttons.as_ptr(),
        nDefaultButton: IDCANCEL.0,
        pfCallback: Some(callback),
        lpCallbackData: state as *const ConfirmationState as isize,
        ..Default::default()
    };
    let mut selected = 0;
    unsafe { TaskDialogIndirect(&config, Some(&mut selected), None, None) }
        .context("creating native confirmation")?;
    #[cfg(feature = "test-support")]
    if request.fault == Some(WindowsNativeConfirmationTestFault::MissingDestruction) {
        state.destroyed.set(false);
    }
    ensure!(
        state.created.get() && state.destroyed.get(),
        "native confirmation destruction was not acknowledged"
    );
    match selected {
        _ if state.cancelled.get() => Ok(WindowsNativeConfirmationOutcome::Cancelled),
        value if value == IDCANCEL.0 => Ok(WindowsNativeConfirmationOutcome::Cancelled),
        value if value == IDOK.0 => Ok(WindowsNativeConfirmationOutcome::Confirmed),
        _ => anyhow::bail!("native confirmation returned an unknown choice"),
    }
}
