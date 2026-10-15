#![allow(missing_docs)]

use super::{
    WindowsWindowInner, events::get_client_area_insets, util::get_window_long,
    window::WindowBorderOffset,
};
use crate::window_minimum::{
    checked_minimum_pixels, checked_tracking_minimum, minimum_update_channel,
};
use crate::{
    InitialWindowState, Pixels, Size, WindowMinimumUpdate, WindowMinimumUpdateError as Error,
    WindowMinimumUpdateFailure as Failure, WindowMinimumUpdateOutcome as Outcome, px, size,
};
use std::rc::Rc;
use windows::Win32::{
    Foundation::RECT,
    UI::{
        HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow},
        WindowsAndMessaging::*,
    },
};

#[cfg(feature = "test-support")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsWindowMinimumTestFault {
    Refuse,
    Unverified,
}

#[cfg(feature = "test-support")]
thread_local! {
    static FAULT: std::cell::Cell<Option<WindowsWindowMinimumTestFault>> = const { std::cell::Cell::new(None) };
}

#[cfg(feature = "test-support")]
pub fn with_windows_window_minimum_fault_for_test<R>(
    fault: WindowsWindowMinimumTestFault,
    operation: impl FnOnce() -> R,
) -> R {
    struct Restore(Option<WindowsWindowMinimumTestFault>);
    impl Drop for Restore {
        fn drop(&mut self) {
            FAULT.with(|slot| slot.set(self.0));
        }
    }
    let _restore = Restore(FAULT.with(|slot| slot.replace(Some(fault))));
    operation()
}

struct Geometry {
    outer: RECT,
    client: RECT,
    normal: RECT,
    scale: f32,
    border: WindowBorderOffset,
}

impl WindowsWindowInner {
    fn normal_minimum_border(&self) -> windows::core::Result<WindowBorderOffset> {
        let dpi = unsafe { GetDpiForWindow(self.hwnd) };
        if dpi == 0 {
            return Err(windows::core::Error::from_win32());
        }
        if self.hide_title_bar {
            let insets = get_client_area_insets(self.hwnd, false, self.windows_version);
            return Ok(WindowBorderOffset {
                width_offset: insets.left + insets.right,
                height_offset: insets.top + insets.bottom,
            });
        }
        let mut rect = RECT::default();
        unsafe {
            let style = WINDOW_STYLE(get_window_long(self.hwnd, GWL_STYLE) as u32)
                & !(WS_MAXIMIZE | WS_MINIMIZE);
            let extended = WINDOW_EX_STYLE(get_window_long(self.hwnd, GWL_EXSTYLE) as u32);
            AdjustWindowRectExForDpi(
                &mut rect,
                style,
                !GetMenu(self.hwnd).is_invalid(),
                extended,
                dpi,
            )?;
        }
        Ok(WindowBorderOffset {
            width_offset: rect.right - rect.left,
            height_offset: rect.bottom - rect.top,
        })
    }

    fn minimum_geometry(&self) -> windows::core::Result<Geometry> {
        let mut outer = RECT::default();
        let mut client = RECT::default();
        let mut placement = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        unsafe {
            GetWindowRect(self.hwnd, &mut outer)?;
            GetClientRect(self.hwnd, &mut client)?;
            GetWindowPlacement(self.hwnd, &mut placement)?;
        }
        let dpi = unsafe { GetDpiForWindow(self.hwnd) };
        if dpi == 0 {
            return Err(windows::core::Error::from_win32());
        }
        let scale = dpi as f32 / 96.;
        let border = WindowBorderOffset {
            width_offset: (outer.right - outer.left) - (client.right - client.left),
            height_offset: (outer.bottom - outer.top) - (client.bottom - client.top),
        };
        Ok(Geometry {
            outer,
            client,
            normal: placement.rcNormalPosition,
            scale,
            border,
        })
    }

    pub(super) fn update_minimum_size(
        self: &Rc<Self>,
        minimum: Size<Pixels>,
    ) -> Result<WindowMinimumUpdate, Error> {
        {
            let state = self.native_operation.borrow();
            if state.destroy_requested || state.native_destroyed || self.native_close_requested() {
                return Err(Error::Closed);
            }
            if state.active
                || self.confirmation.borrow().is_some()
                || self.recoverable_destruction.borrow().is_some()
            {
                return Err(Error::Busy);
            }
        }
        {
            let state = self.state.borrow();
            checked_minimum_pixels(
                minimum,
                state.scale_factor,
                (
                    state.border_offset.width_offset,
                    state.border_offset.height_offset,
                ),
            )?;
        }
        self.native_operation.borrow_mut().active = true;
        let (sender, completion) = minimum_update_channel();
        let owner = self.clone();
        #[cfg(feature = "test-support")]
        let fault = FAULT.with(std::cell::Cell::get);
        self.executor
            .spawn(async move {
                let previous = owner.state.borrow().min_size;
                let mut outcome = if owner.minimum_owner_closed() {
                    Outcome::NotApplied(Failure::Closed)
                } else {
                    #[cfg(feature = "test-support")]
                    if fault == Some(WindowsWindowMinimumTestFault::Refuse) {
                        Outcome::NotApplied(Failure::NativeFailure)
                    } else {
                        let outcome = owner.apply_minimum(minimum);
                        if fault == Some(WindowsWindowMinimumTestFault::Unverified)
                            && matches!(outcome, Outcome::Applied { .. })
                        {
                            Outcome::Unverified
                        } else {
                            outcome
                        }
                    }
                    #[cfg(not(feature = "test-support"))]
                    owner.apply_minimum(minimum)
                };
                if owner.minimum_owner_closed() {
                    outcome = Outcome::NotApplied(Failure::Closed);
                }
                if !matches!(outcome, Outcome::Applied { .. }) {
                    owner.state.borrow_mut().min_size = previous;
                }
                owner.native_operation.borrow_mut().active = false;
                if owner.minimum_owner_closed() && owner.destroy_native_window().is_err() {
                    outcome = Outcome::Unverified;
                }
                let _ = sender.send(outcome);
            })
            .detach();
        Ok(completion)
    }

    fn minimum_owner_closed(&self) -> bool {
        let state = self.native_operation.borrow();
        state.destroy_requested || state.native_destroyed || self.native_close_requested()
    }

    fn apply_minimum(&self, minimum: Size<Pixels>) -> Outcome {
        if unsafe { IsIconic(self.hwnd) }.as_bool() {
            return Outcome::NotApplied(Failure::Minimized);
        }
        if self.state.borrow().is_fullscreen() {
            return Outcome::NotApplied(Failure::UnsupportedState);
        }
        let Ok(before) = self.minimum_geometry() else {
            return Outcome::Unverified;
        };
        let Ok(required) = checked_minimum_pixels(
            minimum,
            before.scale,
            (before.border.width_offset, before.border.height_offset),
        ) else {
            return Outcome::NotApplied(Failure::NativeFailure);
        };
        let maximized = self.minimum_initial_state() == Some(InitialWindowState::Maximized)
            || unsafe { IsZoomed(self.hwnd) }.as_bool();
        let adequate = before.client.right - before.client.left >= required.width.0
            && before.client.bottom - before.client.top >= required.height.0;
        if maximized {
            let Ok(normal_border) = self.normal_minimum_border() else {
                return Outcome::Unverified;
            };
            let normal_width = i64::from(before.normal.right)
                - i64::from(before.normal.left)
                - i64::from(normal_border.width_offset);
            let normal_height = i64::from(before.normal.bottom)
                - i64::from(before.normal.top)
                - i64::from(normal_border.height_offset);
            if !adequate
                || normal_width < i64::from(required.width.0)
                || normal_height < i64::from(required.height.0)
            {
                return Outcome::NotApplied(Failure::InsufficientSpace);
            }
        }
        self.state.borrow_mut().min_size = Some(minimum);
        let changed = if !adequate && !maximized {
            let width = (i64::from(before.outer.right) - i64::from(before.outer.left))
                .max(i64::from(required.width.0) + i64::from(before.border.width_offset));
            let height = (i64::from(before.outer.bottom) - i64::from(before.outer.top))
                .max(i64::from(required.height.0) + i64::from(before.border.height_offset));
            if i32::try_from(width).is_err()
                || i32::try_from(height).is_err()
                || i32::try_from(i64::from(before.outer.left) + width).is_err()
                || i32::try_from(i64::from(before.outer.top) + height).is_err()
            {
                return Outcome::NotApplied(Failure::InsufficientSpace);
            }
            unsafe {
                SetWindowPos(
                    self.hwnd,
                    None,
                    0,
                    0,
                    width as i32,
                    height as i32,
                    SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
                )
            }
            .is_ok()
        } else {
            true
        };
        let Ok(after) = self.minimum_geometry() else {
            return Outcome::Unverified;
        };
        let Ok(required) = checked_minimum_pixels(
            minimum,
            after.scale,
            (after.border.width_offset, after.border.height_offset),
        ) else {
            return Outcome::NotApplied(Failure::NativeFailure);
        };
        if !changed
            || after.outer.left != before.outer.left
            || after.outer.top != before.outer.top
            || after.client.right - after.client.left < required.width.0
            || after.client.bottom - after.client.top < required.height.0
            || unsafe { IsIconic(self.hwnd) }.as_bool()
            || unsafe { IsZoomed(self.hwnd) }.as_bool() != maximized
        {
            return Outcome::NotApplied(Failure::NativeFailure);
        }
        Outcome::Applied {
            minimum,
            client_size: size(
                px((after.client.right - after.client.left) as f32 / after.scale),
                px((after.client.bottom - after.client.top) as f32 / after.scale),
            ),
        }
    }

    pub(super) fn prepare_initial_minimum(
        &self,
        minimum: Size<Pixels>,
        initial: InitialWindowState,
        placement: &mut WINDOWPLACEMENT,
    ) -> anyhow::Result<()> {
        let state = self.state.borrow();
        let required = checked_tracking_minimum(
            minimum,
            state.scale_factor,
            (
                state.border_offset.width_offset,
                state.border_offset.height_offset,
            ),
        )?;
        let border = state.border_offset;
        drop(state);
        let width = i64::from(placement.rcNormalPosition.right)
            - i64::from(placement.rcNormalPosition.left);
        let height = i64::from(placement.rcNormalPosition.bottom)
            - i64::from(placement.rcNormalPosition.top);
        let required_width = i64::from(required.width.0) + i64::from(border.width_offset);
        let required_height = i64::from(required.height.0) + i64::from(border.height_offset);
        if initial == InitialWindowState::Windowed {
            placement.rcNormalPosition.right = i32::try_from(
                i64::from(placement.rcNormalPosition.left) + width.max(required_width),
            )?;
            placement.rcNormalPosition.bottom = i32::try_from(
                i64::from(placement.rcNormalPosition.top) + height.max(required_height),
            )?;
        } else if initial == InitialWindowState::Maximized {
            let normal_border = self.normal_minimum_border()?;
            let required_width =
                i64::from(required.width.0) + i64::from(normal_border.width_offset);
            let required_height =
                i64::from(required.height.0) + i64::from(normal_border.height_offset);
            let actual = self.minimum_geometry()?;
            anyhow::ensure!(
                width >= required_width
                    && height >= required_height
                    && actual.client.right >= required.width.0
                    && actual.client.bottom >= required.height.0,
                "initial maximized window cannot satisfy its declared client minimum"
            );
        }
        Ok(())
    }

    pub(super) fn verify_initial_minimum(&self, minimum: Size<Pixels>) -> anyhow::Result<()> {
        let actual = self.minimum_geometry()?;
        let required = checked_tracking_minimum(
            minimum,
            actual.scale,
            (actual.border.width_offset, actual.border.height_offset),
        )?;
        anyhow::ensure!(
            actual.client.right - actual.client.left >= required.width.0
                && actual.client.bottom - actual.client.top >= required.height.0,
            "initial window cannot satisfy its declared client minimum"
        );
        Ok(())
    }
}
