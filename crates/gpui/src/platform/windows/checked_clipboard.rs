use std::ffi::c_void;

use windows::{
    Win32::{
        Foundation::{
            ERROR_SUCCESS, GetLastError, GlobalFree, HANDLE, HGLOBAL, HWND, SetLastError,
        },
        System::{
            DataExchange::{
                CloseClipboard, CountClipboardFormats, EmptyClipboard, EnumClipboardFormats,
                GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
                OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
            },
            Memory::{
                GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
            },
            Ole::CF_UNICODETEXT,
        },
    },
    core::w,
};

use crate::ImageFormat;
use crate::checked_clipboard::{ClipboardError, Format, NativeClipboard};

pub(crate) struct WindowsClipboard {
    owner: HWND,
    formats: [(Format, u32); 8],
}

impl WindowsClipboard {
    pub(crate) fn new(owner: HWND) -> Result<Self, ClipboardError> {
        let register = |name| {
            let value = unsafe { RegisterClipboardFormatW(name) };
            if value == 0 {
                Err(ClipboardError::Unavailable)
            } else {
                Ok(value)
            }
        };
        Ok(Self {
            owner,
            formats: [
                (Format::Text, CF_UNICODETEXT.0 as u32),
                (Format::Metadata, register(w!("GPUI internal metadata"))?),
                (Format::TextHash, register(w!("GPUI internal text hash"))?),
                (
                    Format::ImageMetadata,
                    register(w!("GPUI checked image representation"))?,
                ),
                (Format::Image(ImageFormat::Png), register(w!("PNG"))?),
                (Format::Image(ImageFormat::Gif), register(w!("GIF"))?),
                (Format::Image(ImageFormat::Jpeg), register(w!("JFIF"))?),
                (
                    Format::Image(ImageFormat::Svg),
                    register(w!("image/svg+xml"))?,
                ),
            ],
        })
    }

    fn number(&self, format: Format) -> Result<u32, ClipboardError> {
        self.formats
            .iter()
            .find_map(|(candidate, number)| (*candidate == format).then_some(*number))
            .ok_or(ClipboardError::Unsupported)
    }

    pub(crate) fn data_number(
        &mut self,
        number: u32,
        maximum: usize,
    ) -> Result<LockedData, ClipboardError> {
        let handle = unsafe { GetClipboardData(number) }.map_err(|_| ClipboardError::Read)?;
        let global = HGLOBAL(handle.0);
        let size = unsafe { GlobalSize(global) };
        if size == 0 {
            return Err(ClipboardError::Malformed);
        }
        if size > maximum || size > isize::MAX as usize {
            return Err(ClipboardError::OverLimit);
        }
        let pointer = unsafe { GlobalLock(global) };
        if pointer.is_null() {
            return Err(ClipboardError::Read);
        }
        Ok(LockedData {
            global,
            pointer,
            size,
        })
    }

    pub(crate) fn publish_number(
        &mut self,
        number: u32,
        mut buffer: OwnedBuffer,
        error: ClipboardError,
    ) -> Result<(), ClipboardError> {
        unsafe {
            SetLastError(ERROR_SUCCESS);
        }
        let _ = unsafe { GlobalUnlock(buffer.global) };
        if unsafe { GetLastError() } != ERROR_SUCCESS {
            return Err(error);
        }
        buffer.locked = false;
        unsafe { SetClipboardData(number, Some(HANDLE(buffer.global.0))) }.map_err(|_| error)?;
        buffer.transferred = true;
        Ok(())
    }
}

pub(crate) struct LockedData {
    global: HGLOBAL,
    pointer: *mut c_void,
    size: usize,
}

impl AsRef<[u8]> for LockedData {
    fn as_ref(&self) -> &[u8] {
        // GlobalSize was admitted before locking; the clipboard session outlives this guard.
        unsafe { std::slice::from_raw_parts(self.pointer.cast(), self.size) }
    }
}

impl Drop for LockedData {
    fn drop(&mut self) {
        let _ = unsafe { GlobalUnlock(self.global) };
    }
}

pub(crate) struct OwnedBuffer {
    global: HGLOBAL,
    pointer: *mut c_void,
    size: usize,
    locked: bool,
    transferred: bool,
}

impl AsRef<[u8]> for OwnedBuffer {
    fn as_ref(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.pointer.cast(), self.size) }
    }
}

impl AsMut<[u8]> for OwnedBuffer {
    fn as_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.pointer.cast(), self.size) }
    }
}

impl Drop for OwnedBuffer {
    fn drop(&mut self) {
        if !self.transferred {
            if self.locked {
                let _ = unsafe { GlobalUnlock(self.global) };
            }
            let _ = unsafe { GlobalFree(Some(self.global)) };
        }
    }
}

impl NativeClipboard for WindowsClipboard {
    type Data = LockedData;
    type Buffer = OwnedBuffer;

    fn open(&mut self) -> Result<(), ClipboardError> {
        unsafe { OpenClipboard(Some(self.owner)) }.map_err(|_| ClipboardError::Ownership)
    }

    fn close(&mut self) -> Result<(), ClipboardError> {
        unsafe { CloseClipboard() }.map_err(|_| ClipboardError::Close)
    }

    fn sequence(&self) -> Result<u32, ClipboardError> {
        let sequence = unsafe { GetClipboardSequenceNumber() };
        if sequence == 0 {
            Err(ClipboardError::Read)
        } else {
            Ok(sequence)
        }
    }

    fn first_supported(&mut self) -> Result<Format, ClipboardError> {
        unsafe {
            SetLastError(ERROR_SUCCESS);
        }
        let count = unsafe { CountClipboardFormats() };
        if count == 0 {
            if unsafe { GetLastError() } != ERROR_SUCCESS {
                return Err(ClipboardError::Read);
            }
            return Err(ClipboardError::NoValue);
        }
        if count < 0 {
            return Err(ClipboardError::Read);
        }
        let mut number = 0;
        for _ in 0..count {
            number = unsafe { EnumClipboardFormats(number) };
            if number == 0 {
                return Err(ClipboardError::Read);
            }
            if let Some((format, _)) = self.formats.iter().find(|(format, value)| {
                *value == number && matches!(format, Format::Text | Format::Image(_))
            }) {
                return Ok(*format);
            }
        }
        Err(ClipboardError::Unsupported)
    }

    fn available(&mut self, format: Format) -> Result<bool, ClipboardError> {
        Ok(unsafe { IsClipboardFormatAvailable(self.number(format)?) }.is_ok())
    }

    fn data(&mut self, format: Format, maximum: usize) -> Result<LockedData, ClipboardError> {
        self.data_number(self.number(format)?, maximum)
    }

    fn allocate(&mut self, bytes: usize) -> Result<OwnedBuffer, ClipboardError> {
        if bytes == 0 || bytes > isize::MAX as usize {
            return Err(ClipboardError::OverLimit);
        }
        let global = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes) }
            .map_err(|_| ClipboardError::Allocation)?;
        let mut buffer = OwnedBuffer {
            global,
            pointer: std::ptr::null_mut(),
            size: unsafe { GlobalSize(global) },
            locked: false,
            transferred: false,
        };
        if buffer.size < bytes || buffer.size > isize::MAX as usize {
            return Err(ClipboardError::Allocation);
        }
        buffer.pointer = unsafe { GlobalLock(global) };
        if buffer.pointer.is_null() {
            return Err(ClipboardError::Allocation);
        }
        buffer.locked = true;
        Ok(buffer)
    }

    fn empty(&mut self) -> Result<(), ClipboardError> {
        unsafe { EmptyClipboard() }.map_err(|_| ClipboardError::Ownership)
    }

    fn publish(&mut self, format: Format, buffer: OwnedBuffer) -> Result<(), ClipboardError> {
        let error = ClipboardError::Write(format.representation());
        self.publish_number(self.number(format)?, buffer, error)
    }
}
