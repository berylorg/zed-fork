use crate::checked_clipboard::ClipboardError;
use std::{
    marker::PhantomData,
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};
use windows::{
    Win32::{
        Foundation::{GetLastError, HANDLE},
        System::{
            StationsAndDesktops::{
                CloseDesktop, CloseWindowStation, CreateDesktopExW, CreateWindowStationW,
                DESKTOP_CONTROL_FLAGS, DESKTOP_CREATEWINDOW, DESKTOP_READOBJECTS,
                DESKTOP_WRITEOBJECTS, GetProcessWindowStation, GetThreadDesktop,
                GetUserObjectInformationW, HDESK, HWINSTA, SetProcessWindowStation,
                SetThreadDesktop, UOI_NAME,
            },
            Threading::GetCurrentThreadId,
        },
    },
    core::{PCWSTR, w},
};

fn checked<T>(
    stage: &'static str,
    result: windows::core::Result<T>,
    error: ClipboardError,
) -> Result<T, ClipboardError> {
    result.map_err(|native_error| {
        let code = unsafe { GetLastError() }.0;
        eprintln!(
            "clipboard isolation stage={stage}, hresult={:#x}, last_win32={code}",
            native_error.code().0
        );
        error
    })
}

fn station_name(station: HWINSTA) -> Result<[u16; 256], ClipboardError> {
    let mut name = [0u16; 256];
    let mut bytes = 0;
    checked(
        "station.name",
        unsafe {
            GetUserObjectInformationW(
                HANDLE(station.0),
                UOI_NAME,
                Some(name.as_mut_ptr().cast()),
                std::mem::size_of_val(&name) as u32,
                Some(&mut bytes),
            )
        },
        ClipboardError::Ownership,
    )?;
    if bytes == 0
        || bytes % 2 != 0
        || bytes as usize > std::mem::size_of_val(&name)
        || !name[..bytes as usize / 2].contains(&0)
    {
        return Err(ClipboardError::Ownership);
    }
    Ok(name)
}

pub(crate) struct ClipboardWindowStation {
    original_station: HWINSTA,
    original_desktop: HDESK,
    station: Option<HWINSTA>,
    desktop: Option<HDESK>,
    finished: bool,
    same_thread: PhantomData<Rc<()>>,
}

impl ClipboardWindowStation {
    pub(crate) fn enter() -> Result<Self, ClipboardError> {
        let original_station = checked(
            "station.capture",
            unsafe { GetProcessWindowStation() },
            ClipboardError::Ownership,
        )?;
        let original_desktop = checked(
            "desktop.capture",
            unsafe { GetThreadDesktop(GetCurrentThreadId()) },
            ClipboardError::Ownership,
        )?;
        let original_name = station_name(original_station)?;
        let mut guard = Self {
            original_station,
            original_desktop,
            station: None,
            desktop: None,
            finished: false,
            same_thread: PhantomData,
        };
        // Create-only prevents attachment to any pre-existing station with the same name.
        const CWF_CREATE_ONLY: u32 = 1;
        const STATION_ACCESS: u32 = 0x0002 | 0x0004 | 0x0008 | 0x0020;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ClipboardError::Ownership)?
            .as_nanos();
        let name = format!(
            "GPUIClipboardQualification-{}-{timestamp}",
            std::process::id()
        );
        let name = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let station = checked(
            "station.create",
            unsafe {
                CreateWindowStationW(PCWSTR(name.as_ptr()), CWF_CREATE_ONLY, STATION_ACCESS, None)
            },
            ClipboardError::Ownership,
        )?;
        guard.station = Some(station);
        checked(
            "station.assign",
            unsafe { SetProcessWindowStation(station) },
            ClipboardError::Ownership,
        )?;
        let observed = checked(
            "station.verify",
            unsafe { GetProcessWindowStation() },
            ClipboardError::Ownership,
        )?;
        if observed != station
            || observed == original_station
            || station_name(observed)? == original_name
            || station_name(observed)? != station_name(station)?
        {
            return Err(ClipboardError::Ownership);
        }
        let desktop = checked(
            "desktop.create",
            unsafe {
                CreateDesktopExW(
                    w!("GPUIClipboardQualification"),
                    PCWSTR::null(),
                    None,
                    DESKTOP_CONTROL_FLAGS::default(),
                    DESKTOP_CREATEWINDOW.0 | DESKTOP_READOBJECTS.0 | DESKTOP_WRITEOBJECTS.0,
                    None,
                    512,
                    None,
                )
            },
            ClipboardError::Ownership,
        )?;
        guard.desktop = Some(desktop);
        checked(
            "desktop.assign",
            unsafe { SetThreadDesktop(desktop) },
            ClipboardError::Ownership,
        )?;
        let observed_desktop = checked(
            "desktop.verify",
            unsafe { GetThreadDesktop(GetCurrentThreadId()) },
            ClipboardError::Ownership,
        )?;
        if observed_desktop != desktop || observed_desktop == original_desktop {
            return Err(ClipboardError::Ownership);
        }
        eprintln!("clipboard isolation: private station and desktop verified");
        Ok(guard)
    }

    fn restore(&mut self) -> Result<(), ClipboardError> {
        if self.station.is_none() && self.desktop.is_none() {
            return Ok(());
        }
        // The original thread desktop belongs to the original process station.
        checked(
            "station.restore",
            unsafe { SetProcessWindowStation(self.original_station) },
            ClipboardError::Close,
        )?;
        checked(
            "desktop.restore",
            unsafe { SetThreadDesktop(self.original_desktop) },
            ClipboardError::Close,
        )?;
        let station = checked(
            "station.restored.verify",
            unsafe { GetProcessWindowStation() },
            ClipboardError::Close,
        )?;
        let desktop = checked(
            "desktop.restored.verify",
            unsafe { GetThreadDesktop(GetCurrentThreadId()) },
            ClipboardError::Close,
        )?;
        if station != self.original_station || desktop != self.original_desktop {
            return Err(ClipboardError::Close);
        }
        if let Some(desktop) = self.desktop {
            checked(
                "desktop.close",
                unsafe { CloseDesktop(desktop) },
                ClipboardError::Close,
            )?;
            self.desktop = None;
        }
        if let Some(station) = self.station {
            checked(
                "station.close",
                unsafe { CloseWindowStation(station) },
                ClipboardError::Close,
            )?;
            self.station = None;
        }
        eprintln!(
            "clipboard isolation: original station and desktop restored; private handles closed"
        );
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Result<(), ClipboardError> {
        let result = self.restore();
        self.finished = true;
        result
    }
}

impl Drop for ClipboardWindowStation {
    fn drop(&mut self) {
        if !self.finished {
            if let Err(error) = self.restore() {
                eprintln!("clipboard isolation cleanup failed: {error}");
            }
        }
    }
}
