#![cfg(target_os = "windows")]

#[path = "../src/platform/windows/input_handler_access.rs"]
mod input_handler_access;

use input_handler_access::InputHandlerAccess;
use std::{cell::RefCell, rc::Rc};

struct Handler(&'static str, Rc<RefCell<Vec<&'static str>>>);

impl Drop for Handler {
    fn drop(&mut self) {
        self.1.borrow_mut().push(self.0);
    }
}

#[test]
fn unchanged_handler_is_restored_once_and_nested_access_is_refused() {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut slot = InputHandlerAccess::default();
    assert!(slot.register(Handler("original", drops.clone())).is_none());
    assert!(slot.begin_access());
    assert!(!slot.begin_access());
    let original = slot.take_current();
    assert!(slot.finish_access(original, true).is_none());
    assert!(drops.borrow().is_empty());
    assert!(slot.begin_access());
    let original = slot.take_current().unwrap();
    assert_eq!(original.0, "original");
    drop(slot.finish_access(Some(original), false));
    assert_eq!(&*drops.borrow(), &["original"]);
    assert!(slot.begin_access());
    assert!(slot.take_current().is_none());
    assert!(slot.finish_access(None, true).is_none());
}

#[test]
fn newer_registration_survives_old_handler_return() {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut slot = InputHandlerAccess::default();
    assert!(slot.register(Handler("old", drops.clone())).is_none());
    assert!(slot.begin_access());
    let old = slot.take_current();
    assert!(slot.register(Handler("new", drops.clone())).is_none());
    drop(slot.finish_access(old, true));
    assert_eq!(&*drops.borrow(), &["old"]);
    let new = slot.take().unwrap();
    assert_eq!(new.0, "new");
    drop(new);
    assert_eq!(&*drops.borrow(), &["old", "new"]);
}

#[test]
fn explicit_clearing_of_extracted_slot_is_preserved() {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut slot = InputHandlerAccess::default();
    assert!(slot.register(Handler("old", drops.clone())).is_none());
    assert!(slot.begin_access());
    let old = slot.take_current();
    assert!(slot.take().is_none());
    drop(slot.finish_access(old, true));
    assert!(slot.take().is_none());
    assert_eq!(&*drops.borrow(), &["old"]);
}

#[test]
fn synchronization_registration_becomes_current_before_use() {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut slot = InputHandlerAccess::default();
    assert!(
        slot.register(Handler("painted-old", drops.clone()))
            .is_none()
    );
    assert!(slot.begin_access());
    drop(slot.take());
    assert!(
        slot.register(Handler("focused-new", drops.clone()))
            .is_none()
    );
    let current = slot.take_current().unwrap();
    assert_eq!(current.0, "focused-new");
    assert!(slot.finish_access(Some(current), true).is_none());
    assert_eq!(&*drops.borrow(), &["painted-old"]);
    drop(slot.take());
    assert_eq!(&*drops.borrow(), &["painted-old", "focused-new"]);
}

#[test]
fn refusal_without_extraction_releases_admission_and_preserves_registration() {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut slot = InputHandlerAccess::default();
    assert!(slot.register(Handler("retained", drops.clone())).is_none());
    assert!(slot.begin_access());
    assert!(slot.finish_access(None, true).is_none());
    assert!(slot.begin_access());
    let retained = slot.take_current().unwrap();
    assert_eq!(retained.0, "retained");
    drop(slot.finish_access(Some(retained), false));
    assert_eq!(&*drops.borrow(), &["retained"]);
}

#[test]
fn displaced_values_can_be_dropped_after_slot_borrow_ends() {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let slot = RefCell::new(InputHandlerAccess::default());
    assert!(
        slot.borrow_mut()
            .register(Handler("first", drops.clone()))
            .is_none()
    );
    let displaced = slot.borrow_mut().register(Handler("second", drops.clone()));
    assert!(drops.borrow().is_empty());
    assert!(slot.try_borrow_mut().is_ok());
    drop(displaced);
    assert_eq!(&*drops.borrow(), &["first"]);
}
