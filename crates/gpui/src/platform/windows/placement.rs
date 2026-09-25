#![allow(missing_docs)]

use std::{
    ops::ControlFlow,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
};

use anyhow::{Context as _, Result, ensure};
use uuid::Uuid;
use windows::Win32::{
    Foundation::{HWND, LPARAM, RECT},
    Graphics::Gdi::{
        EnumDisplayMonitors, HDC, HMONITOR, MONITOR_DEFAULTTONULL, MonitorFromRect,
        MonitorFromWindow,
    },
    UI::{
        HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI},
        WindowsAndMessaging::USER_DEFAULT_SCREEN_DPI,
    },
};
use windows::core::BOOL;

use super::display::{WindowsDisplay, generate_uuid, get_monitor_info};
use crate::{Bounds, DevicePixels, DisplayId, Pixels, point, size};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowsWindowPlacementMonitor {
    native_identity: usize,
    device_name: [u16; 32],
    physical_bounds: Bounds<DevicePixels>,
    work_area: Bounds<DevicePixels>,
    dpi: u32,
}

impl WindowsWindowPlacementMonitor {
    #[cfg(feature = "test-support")]
    pub fn with_test_work_area(mut self, work_area: Bounds<DevicePixels>) -> Self {
        self.work_area = work_area;
        self
    }

    #[cfg(feature = "test-support")]
    pub fn with_test_dpi(mut self, dpi: u32) -> Self {
        self.dpi = dpi;
        self
    }

    #[cfg(feature = "test-support")]
    pub fn with_test_native_identity(mut self, native_identity: usize) -> Self {
        self.native_identity = native_identity;
        self
    }

    pub fn visit(mut visitor: impl FnMut(Self) -> Result<ControlFlow<()>>) -> Result<()> {
        visit_monitors(&mut |handle, _| visitor(Self::capture(handle)?))
    }

    pub fn native_identity(&self) -> usize {
        self.native_identity
    }
    pub fn device_name(&self) -> &[u16] {
        let end = self
            .device_name
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(self.device_name.len());
        &self.device_name[..end]
    }
    pub fn uuid(&self) -> Uuid {
        generate_uuid(&self.device_name)
    }
    pub fn physical_bounds(&self) -> Bounds<DevicePixels> {
        self.physical_bounds
    }
    pub fn work_area(&self) -> Bounds<DevicePixels> {
        self.work_area
    }
    pub fn scale_factor(&self) -> f32 {
        self.dpi as f32 / USER_DEFAULT_SCREEN_DPI as f32
    }

    pub fn outer_placement(
        &self,
        bounds: Bounds<Pixels>,
        tool_window: bool,
    ) -> Result<WindowsOuterWindowPlacement> {
        WindowsOuterWindowPlacement::new(
            bounds,
            self.scale_factor(),
            self.physical_bounds,
            self.work_area,
            tool_window,
        )
    }

    fn handle(&self) -> HMONITOR {
        HMONITOR(self.native_identity as *mut _)
    }

    fn capture(handle: HMONITOR) -> Result<Self> {
        ensure!(
            !handle.is_invalid(),
            "prepared monitor has no native identity"
        );
        let info =
            get_monitor_info(handle).context("reading prepared monitor identity and geometry")?;
        let physical_bounds = rect_bounds(info.monitorInfo.rcMonitor)?;
        let work_area = rect_bounds(info.monitorInfo.rcWork)?;
        validate_work_area(physical_bounds, work_area)?;
        ensure!(
            info.szDevice[0] != 0,
            "prepared monitor has no device identity"
        );
        let mut dpi_x = 0;
        let mut dpi_y = 0;
        unsafe { GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }
            .context("reading prepared monitor DPI")?;
        ensure!(
            dpi_x > 0 && dpi_x == dpi_y,
            "prepared monitor has invalid or asymmetric DPI"
        );
        Ok(Self {
            native_identity: handle.0 as usize,
            device_name: info.szDevice,
            physical_bounds,
            work_area,
            dpi: dpi_x,
        })
    }

    pub(crate) fn validate(&self) -> Result<WindowsDisplay> {
        let mut display = None;
        visit_monitors(&mut |handle, index| {
            if handle == self.handle() {
                ensure!(
                    Self::capture(handle)? == *self,
                    "prepared monitor identity, geometry or DPI changed"
                );
                display = Some(WindowsDisplay::from_prepared(
                    handle,
                    DisplayId(index),
                    self.physical_bounds,
                    self.scale_factor(),
                    self.uuid(),
                ));
                return Ok(ControlFlow::Break(()));
            }
            Ok(ControlFlow::Continue(()))
        })?;
        display.context("prepared monitor disconnected")
    }

    pub(crate) fn validate_window(&self, hwnd: HWND) -> Result<()> {
        self.validate()?;
        ensure!(
            unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL) } == self.handle(),
            "native window was created on another monitor"
        );
        ensure!(
            unsafe { GetDpiForWindow(hwnd) } == self.dpi,
            "native window DPI differs from prepared monitor"
        );
        Ok(())
    }

    pub(crate) fn validate_placement(&self, placement: WindowsOuterWindowPlacement) -> Result<()> {
        let rect = bounds_rect(placement.screen_bounds)?;
        ensure!(
            rect.left != i32::MIN && rect.top != i32::MIN,
            "outer bounds conflict with the native default-position sentinel"
        );
        ensure!(
            unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONULL) } == self.handle(),
            "outer bounds do not select the prepared monitor"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowsOuterWindowPlacement {
    screen_bounds: Bounds<DevicePixels>,
    workspace_bounds: Bounds<DevicePixels>,
}

impl WindowsOuterWindowPlacement {
    pub fn new(
        bounds: Bounds<Pixels>,
        scale_factor: f32,
        monitor_bounds: Bounds<DevicePixels>,
        work_area: Bounds<DevicePixels>,
        tool_window: bool,
    ) -> Result<Self> {
        validate_work_area(monitor_bounds, work_area)?;
        ensure!(
            scale_factor.is_finite() && scale_factor > 0.,
            "outer placement scale must be finite and positive"
        );
        let [x, y, width, height] = [
            bounds.origin.x.0,
            bounds.origin.y.0,
            bounds.size.width.0,
            bounds.size.height.0,
        ];
        ensure!(
            [x, y, width, height].iter().all(|value| value.is_finite())
                && width > 0.
                && height > 0.,
            "outer bounds must be finite with positive size"
        );
        let scale = f64::from(scale_factor);
        let screen = RECT {
            left: checked_coordinate(f64::from(x) * scale)?,
            top: checked_coordinate(f64::from(y) * scale)?,
            right: checked_coordinate((f64::from(x) + f64::from(width)) * scale)?,
            bottom: checked_coordinate((f64::from(y) + f64::from(height)) * scale)?,
        };
        let screen_bounds = rect_bounds(screen)?;
        let (dx, dy) = if tool_window {
            (0, 0)
        } else {
            (
                i64::from(work_area.origin.x.0) - i64::from(monitor_bounds.origin.x.0),
                i64::from(work_area.origin.y.0) - i64::from(monitor_bounds.origin.y.0),
            )
        };
        let workspace_bounds = rect_bounds(RECT {
            left: i32::try_from(i64::from(screen.left) - dx)?,
            top: i32::try_from(i64::from(screen.top) - dy)?,
            right: i32::try_from(i64::from(screen.right) - dx)?,
            bottom: i32::try_from(i64::from(screen.bottom) - dy)?,
        })?;
        Ok(Self {
            screen_bounds,
            workspace_bounds,
        })
    }

    pub fn screen_bounds(&self) -> Bounds<DevicePixels> {
        self.screen_bounds
    }
    pub fn workspace_bounds(&self) -> Bounds<DevicePixels> {
        self.workspace_bounds
    }
}

fn checked_coordinate(value: f64) -> Result<i32> {
    let value = value.round();
    ensure!(
        value.is_finite() && value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX),
        "outer bounds exceed native coordinate range"
    );
    Ok(value as i32)
}

fn rect_bounds(rect: RECT) -> Result<Bounds<DevicePixels>> {
    let width = rect
        .right
        .checked_sub(rect.left)
        .context("native rectangle width overflow")?;
    let height = rect
        .bottom
        .checked_sub(rect.top)
        .context("native rectangle height overflow")?;
    ensure!(
        width > 0 && height > 0,
        "native rectangle must have positive size"
    );
    Ok(Bounds::new(
        point(DevicePixels(rect.left), DevicePixels(rect.top)),
        size(DevicePixels(width), DevicePixels(height)),
    ))
}

pub(crate) fn bounds_rect(bounds: Bounds<DevicePixels>) -> Result<RECT> {
    ensure!(
        bounds.size.width.0 > 0 && bounds.size.height.0 > 0,
        "native rectangle must have positive size"
    );
    Ok(RECT {
        left: bounds.origin.x.0,
        top: bounds.origin.y.0,
        right: bounds
            .origin
            .x
            .0
            .checked_add(bounds.size.width.0)
            .context("native rectangle right overflow")?,
        bottom: bounds
            .origin
            .y
            .0
            .checked_add(bounds.size.height.0)
            .context("native rectangle bottom overflow")?,
    })
}

fn validate_work_area(monitor: Bounds<DevicePixels>, work: Bounds<DevicePixels>) -> Result<()> {
    let monitor = bounds_rect(monitor)?;
    let work = bounds_rect(work)?;
    ensure!(
        work.left >= monitor.left
            && work.top >= monitor.top
            && work.right <= monitor.right
            && work.bottom <= monitor.bottom,
        "work area lies outside monitor bounds"
    );
    Ok(())
}

fn visit_monitors(visitor: &mut dyn FnMut(HMONITOR, u32) -> Result<ControlFlow<()>>) -> Result<()> {
    struct Visit<'a> {
        visitor: &'a mut dyn FnMut(HMONITOR, u32) -> Result<ControlFlow<()>>,
        index: u32,
        stopped: bool,
        error: Option<anyhow::Error>,
        panic: Option<Box<dyn std::any::Any + Send>>,
    }
    unsafe extern "system" fn callback(
        handle: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        let visit = unsafe { &mut *(data.0 as *mut Visit<'_>) };
        match catch_unwind(AssertUnwindSafe(|| (visit.visitor)(handle, visit.index))) {
            Ok(Ok(ControlFlow::Continue(()))) => {
                if let Some(index) = visit.index.checked_add(1) {
                    visit.index = index;
                    return BOOL(1);
                }
                visit.error = Some(anyhow::anyhow!("monitor enumeration index overflow"));
            }
            Ok(Ok(ControlFlow::Break(()))) => visit.stopped = true,
            Ok(Err(error)) => visit.error = Some(error),
            Err(panic) => visit.panic = Some(panic),
        }
        BOOL(0)
    }
    let mut visit = Visit {
        visitor,
        index: 0,
        stopped: false,
        error: None,
        panic: None,
    };
    let result = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(callback),
            LPARAM(&mut visit as *mut _ as isize),
        )
    };
    if let Some(panic) = visit.panic {
        resume_unwind(panic);
    }
    if let Some(error) = visit.error {
        return Err(error);
    }
    ensure!(
        visit.stopped || result.as_bool(),
        "monitor enumeration failed: {}",
        std::io::Error::last_os_error()
    );
    Ok(())
}
