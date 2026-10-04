use crate::checked_clipboard::{
    ClipboardError as Error, ClipboardLimits as Limits, Format, NativeClipboard,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
#[derive(Default)]
pub(crate) struct State {
    pub(crate) opens: usize,
    pub(crate) closes: usize,
    pub(crate) empties: usize,
    pub(crate) allocations: usize,
    pub(crate) output_allocations: usize,
    pub(crate) locks: usize,
    pub(crate) publications: Vec<Format>,
}

pub(crate) struct Data {
    pub(crate) bytes: Arc<[u8]>,
    pub(crate) state: Rc<RefCell<State>>,
}

impl AsRef<[u8]> for Data {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}
impl Drop for Data {
    fn drop(&mut self) {
        self.state.borrow_mut().locks -= 1;
    }
}

pub(crate) struct Fake {
    pub(crate) state: Rc<RefCell<State>>,
    pub(crate) values: Vec<(Format, Arc<[u8]>)>,
    pub(crate) error: Option<Error>,
    pub(crate) fail_publication: Option<Format>,
    pub(crate) sequence: Cell<u32>,
    pub(crate) sequence_reads: Cell<usize>,
    pub(crate) change_sequence: bool,
    pub(crate) allocation_padding: usize,
    pub(crate) fail_allocation_at: Option<usize>,
    pub(crate) fail_read: Option<Format>,
    pub(crate) fail_empty: bool,
}

impl Default for Fake {
    fn default() -> Self {
        Self {
            state: Rc::default(),
            values: Vec::new(),
            error: None,
            fail_publication: None,
            sequence: Cell::new(17),
            sequence_reads: Cell::new(0),
            change_sequence: false,
            allocation_padding: 0,
            fail_allocation_at: None,
            fail_read: None,
            fail_empty: false,
        }
    }
}

impl Fake {
    pub(crate) fn text(text: &str) -> Self {
        Self {
            values: vec![(Format::Text, wide(text).into())],
            ..Self::default()
        }
    }
}

impl NativeClipboard for Fake {
    type Data = Data;
    type Buffer = Vec<u8>;
    fn open(&mut self) -> Result<(), Error> {
        self.state.borrow_mut().opens += 1;
        if self.error == Some(Error::Ownership) {
            Err(Error::Ownership)
        } else {
            Ok(())
        }
    }
    fn close(&mut self) -> Result<(), Error> {
        assert_eq!(
            self.state.borrow().locks,
            0,
            "all native locks must be released before close"
        );
        self.state.borrow_mut().closes += 1;
        if self.error == Some(Error::Close) {
            Err(Error::Close)
        } else {
            Ok(())
        }
    }
    fn sequence(&self) -> Result<u32, Error> {
        let calls = self.sequence_reads.replace(self.sequence_reads.get() + 1);
        Ok(self.sequence.get() + u32::from(self.change_sequence && calls > 0))
    }
    fn first_supported(&mut self) -> Result<Format, Error> {
        if self.error == Some(Error::Unsupported) {
            return Err(Error::Unsupported);
        }
        self.values
            .iter()
            .find_map(|(format, _)| {
                matches!(format, Format::Text | Format::Image(_)).then_some(*format)
            })
            .ok_or(Error::NoValue)
    }
    fn available(&mut self, format: Format) -> Result<bool, Error> {
        Ok(self
            .values
            .iter()
            .any(|(candidate, _)| *candidate == format))
    }
    fn data(&mut self, format: Format, maximum: usize) -> Result<Data, Error> {
        if self.error == Some(Error::Read) || self.fail_read == Some(format) {
            return Err(Error::Read);
        }
        let bytes = self
            .values
            .iter()
            .find(|(candidate, _)| *candidate == format)
            .ok_or(Error::Read)?
            .1
            .clone();
        if bytes.len() > maximum {
            return Err(Error::OverLimit);
        }
        self.state.borrow_mut().locks += 1;
        Ok(Data {
            bytes,
            state: self.state.clone(),
        })
    }
    fn allocate(&mut self, bytes: usize) -> Result<Vec<u8>, Error> {
        self.state.borrow_mut().allocations += 1;
        if self.error == Some(Error::Allocation)
            || self.fail_allocation_at == Some(self.state.borrow().allocations)
        {
            return Err(Error::Allocation);
        }
        Ok(vec![0; bytes + self.allocation_padding])
    }
    fn empty(&mut self) -> Result<(), Error> {
        self.state.borrow_mut().empties += 1;
        if self.fail_empty {
            return Err(Error::Ownership);
        }
        self.values.clear();
        Ok(())
    }
    fn publish(&mut self, format: Format, buffer: Vec<u8>) -> Result<(), Error> {
        self.state.borrow_mut().publications.push(format);
        if self.fail_publication == Some(format) {
            return Err(Error::Write(format.representation()));
        }
        self.values.push((format, buffer.into()));
        self.sequence.set(self.sequence.get() + 1);
        Ok(())
    }
    fn before_output_allocation(&mut self) -> Result<(), Error> {
        self.state.borrow_mut().output_allocations += 1;
        if self.error == Some(Error::Allocation) {
            Err(Error::Allocation)
        } else {
            Ok(())
        }
    }
}

pub(crate) fn wide(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(Some(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}
pub(crate) fn limits(bytes: usize) -> Limits {
    Limits {
        total_bytes: bytes,
        text_bytes: bytes,
        metadata_bytes: bytes,
        image_bytes: bytes,
    }
}
