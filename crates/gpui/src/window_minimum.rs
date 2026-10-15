#![allow(missing_docs)]

use crate::{DevicePixels, Pixels, Size, size};
use futures::channel::oneshot;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum WindowMinimumUpdateError {
    #[error("window minimum is not representable")]
    InvalidMinimum,
    #[error("another native window operation is active")]
    Busy,
    #[error("window is closing or closed")]
    Closed,
    #[error("checked window minimum updates are unavailable")]
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowMinimumUpdateFailure {
    Minimized,
    InsufficientSpace,
    UnsupportedState,
    NativeFailure,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowMinimumUpdateOutcome {
    Applied {
        minimum: Size<Pixels>,
        client_size: Size<Pixels>,
    },
    NotApplied(WindowMinimumUpdateFailure),
    Unverified,
}

pub struct WindowMinimumUpdate {
    receiver: oneshot::Receiver<WindowMinimumUpdateOutcome>,
}

impl Future for WindowMinimumUpdate {
    type Output = WindowMinimumUpdateOutcome;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.receiver)
            .poll(cx)
            .map(|result| result.unwrap_or(WindowMinimumUpdateOutcome::Unverified))
    }
}

pub(crate) fn minimum_update_channel() -> (
    oneshot::Sender<WindowMinimumUpdateOutcome>,
    WindowMinimumUpdate,
) {
    let (sender, receiver) = oneshot::channel();
    (sender, WindowMinimumUpdate { receiver })
}

pub(crate) fn checked_minimum_pixels(
    minimum: Size<Pixels>,
    scale: f32,
    border: (i32, i32),
) -> Result<Size<DevicePixels>, WindowMinimumUpdateError> {
    checked_dimensions(minimum, scale, border, false)
}

pub(crate) fn checked_tracking_minimum(
    minimum: Size<Pixels>,
    scale: f32,
    border: (i32, i32),
) -> Result<Size<DevicePixels>, WindowMinimumUpdateError> {
    checked_dimensions(minimum, scale, border, true)
}

fn checked_dimensions(
    minimum: Size<Pixels>,
    scale: f32,
    border: (i32, i32),
    allow_zero: bool,
) -> Result<Size<DevicePixels>, WindowMinimumUpdateError> {
    let dimension = |value: Pixels, extra: i32| {
        let scaled = (f64::from(value.0) * f64::from(scale)).ceil();
        if !value.0.is_finite()
            || value.0 < 0.
            || (!allow_zero && value.0 == 0.)
            || !scale.is_finite()
            || scale <= 0.
            || extra < 0
            || !scaled.is_finite()
            || scaled < 0.
            || scaled > f64::from(i32::MAX - extra)
        {
            return Err(WindowMinimumUpdateError::InvalidMinimum);
        }
        Ok(DevicePixels(scaled as i32))
    };
    Ok(size(
        dimension(minimum.width, border.0)?,
        dimension(minimum.height, border.1)?,
    ))
}
