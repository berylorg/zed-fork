#![cfg(target_os = "windows")]
#![allow(dead_code)]

pub use gpui::{ClipboardEntry, ClipboardItem, Image, ImageFormat};

#[path = "../src/checked_clipboard.rs"]
mod checked_clipboard;
#[path = "support/clipboard_window_station.rs"]
mod clipboard_window_station;
#[path = "../src/platform/windows/checked_clipboard.rs"]
mod native;

use checked_clipboard::{
    ClipboardError as Error, ClipboardLimits as Limits, Format, NativeClipboard,
};
use clipboard_window_station::ClipboardWindowStation;
use native::{OwnedBuffer, WindowsClipboard};
use windows::{
    Win32::{
        Foundation::{GetLastError, HWND},
        System::DataExchange::{
            CountClipboardFormats, GetClipboardOwner, GetClipboardSequenceNumber,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
        },
    },
    core::w,
};

struct Owner(HWND);
impl Owner {
    fn new() -> Result<Self, Error> {
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("GPUI clipboard qualification"),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )
        }
        .map(Self)
        .map_err(|error| {
            let native_code = unsafe { GetLastError() }.0;
            eprintln!("clipboard qualification stage=owner.create, hresult={:#x}, last_win32={native_code}", error.code().0);
            Error::Ownership
        })
    }

    fn finish(mut self) -> Result<(), Error> {
        unsafe { DestroyWindow(self.0) }.map_err(|error| {
            let code = unsafe { GetLastError() }.0;
            eprintln!(
                "clipboard qualification stage=owner.destroy, hresult={:#x}, last_win32={code}",
                error.code().0
            );
            Error::Close
        })?;
        self.0 = HWND::default();
        Ok(())
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            if let Err(error) = unsafe { DestroyWindow(self.0) } {
                eprintln!(
                    "clipboard qualification owner cleanup failed: hresult={:#x}",
                    error.code().0
                );
            }
        }
    }
}

fn diagnosed<T>(stage: &'static str, result: Result<T, Error>) -> Result<T, Error> {
    if let Err(error) = &result {
        let native_code = unsafe { GetLastError() }.0;
        eprintln!(
            "clipboard qualification stage={stage}, result={error:?}, last_win32={native_code}"
        );
    }
    result
}

struct DiagnosticClipboard(WindowsClipboard, HWND);

impl DiagnosticClipboard {
    fn new(owner: HWND) -> Result<Self, Error> {
        diagnosed("native.formats", WindowsClipboard::new(owner)).map(|native| Self(native, owner))
    }

    fn probe(&self, stage: &'static str) {
        let sequence = unsafe { GetClipboardSequenceNumber() };
        let owner_matches = unsafe { GetClipboardOwner() }.is_ok_and(|owner| owner == self.1);
        let format_count = unsafe { CountClipboardFormats() };
        eprintln!(
            "clipboard qualification probe={stage}, sequence={sequence}, owner_matches={owner_matches}, format_count={format_count}"
        );
    }
}

impl NativeClipboard for DiagnosticClipboard {
    type Data = native::LockedData;
    type Buffer = OwnedBuffer;
    fn open(&mut self) -> Result<(), Error> {
        let result = diagnosed("native.open", self.0.open());
        if result.is_ok() {
            self.probe("open.held");
        }
        result
    }
    fn close(&mut self) -> Result<(), Error> {
        self.probe("close.before.held");
        let result = diagnosed("native.close", self.0.close());
        if result.is_ok() {
            self.probe("close.after.unheld");
        }
        result
    }
    fn sequence(&self) -> Result<u32, Error> {
        diagnosed("native.sequence", self.0.sequence())
    }
    fn first_supported(&mut self) -> Result<Format, Error> {
        diagnosed("native.select", self.0.first_supported())
    }
    fn available(&mut self, format: Format) -> Result<bool, Error> {
        diagnosed("native.available", self.0.available(format))
    }
    fn data(&mut self, format: Format, maximum: usize) -> Result<Self::Data, Error> {
        self.probe("data.before.held");
        let result = diagnosed("native.data", self.0.data(format, maximum));
        if result.is_ok() {
            self.probe("data.after.held");
        }
        result
    }
    fn allocate(&mut self, bytes: usize) -> Result<Self::Buffer, Error> {
        diagnosed("native.allocate", self.0.allocate(bytes))
    }
    fn empty(&mut self) -> Result<(), Error> {
        let result = diagnosed("native.clear", self.0.empty());
        if result.is_ok() {
            self.probe("clear.after.held");
        }
        result
    }
    fn publish(&mut self, format: Format, buffer: Self::Buffer) -> Result<(), Error> {
        let result = diagnosed("native.publish", self.0.publish(format, buffer));
        if result.is_ok() {
            self.probe("publish.after.held");
        }
        result
    }
}

fn assert_capture_sequence_while_held(
    driver: &mut DiagnosticClipboard,
    sequence: u32,
) -> Result<(), Error> {
    driver.open()?;
    let result = (|| {
        if driver.sequence()? != sequence {
            return Err(Error::SnapshotChanged);
        }
        if !unsafe { GetClipboardOwner() }.is_ok_and(|owner| owner == driver.1) {
            return Err(Error::Ownership);
        }
        Ok(())
    })();
    let close = driver.close();
    close?;
    result
}

fn qualify_private_clipboard() -> Result<(), Error> {
    let owner = diagnosed("owner", Owner::new())?;
    let result = (|| {
        let mut driver = DiagnosticClipboard::new(owner.0)?;
        let limits = Limits {
            total_bytes: 4 * 1024 * 1024,
            text_bytes: 1024 * 1024,
            metadata_bytes: 1024 * 1024,
            image_bytes: 4 * 1024 * 1024,
        };
        let item = ClipboardItem::new_string_with_metadata(
            "clipboard 😀 qualification".into(),
            "opaque qualification metadata".into(),
        );
        diagnosed(
            "text.write",
            checked_clipboard::write(&mut driver, &item, limits),
        )?;
        let snapshot = diagnosed("text.read", checked_clipboard::read(&mut driver, limits))?;
        assert_eq!(snapshot.item, item);
        assert_capture_sequence_while_held(&mut driver, snapshot.sequence)?;
        for format in [Format::Text, Format::Metadata, Format::TextHash] {
            assert!(driver.available(format)?);
        }
        for format in [
            ImageFormat::Png,
            ImageFormat::Gif,
            ImageFormat::Jpeg,
            ImageFormat::Svg,
        ] {
            for length in [1, 3, 5, 9] {
                let item = ClipboardItem::from(Image::from_bytes(
                    format,
                    (1..=length).map(|value| value as u8).collect(),
                ));
                diagnosed(
                    "image.write",
                    checked_clipboard::write(&mut driver, &item, limits),
                )?;
                let snapshot =
                    diagnosed("image.read", checked_clipboard::read(&mut driver, limits))?;
                assert_eq!(snapshot.item, item);
                assert_capture_sequence_while_held(&mut driver, snapshot.sequence)?;
                assert!(driver.available(Format::Image(format))?);
                assert!(driver.available(Format::ImageMetadata)?);
                assert_eq!(unsafe { CountClipboardFormats() }, 2);
            }
        }
        Ok(())
    })();
    owner.finish()?;
    result
}

#[test]
#[ignore = "requires administrator terminal for private window station qualification"]
fn native_ownership_format_publication_on_private_window_station() -> Result<(), Error> {
    let station = ClipboardWindowStation::enter()?;
    let result = qualify_private_clipboard();
    station.finish()?;
    result
}
