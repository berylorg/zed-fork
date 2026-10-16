pub(super) struct InputHandlerAccess<T> {
    handler: Option<T>,
    active: bool,
    changed: bool,
}

impl<T> Default for InputHandlerAccess<T> {
    fn default() -> Self {
        Self {
            handler: None,
            active: false,
            changed: false,
        }
    }
}

impl<T> InputHandlerAccess<T> {
    pub(super) fn begin_access(&mut self) -> bool {
        if self.active {
            return false;
        }
        self.active = true;
        self.changed = false;
        true
    }

    pub(super) fn register(&mut self, handler: T) -> Option<T> {
        self.changed |= self.active;
        self.handler.replace(handler)
    }

    pub(super) fn take(&mut self) -> Option<T> {
        self.changed |= self.active;
        self.handler.take()
    }

    pub(super) fn take_current(&mut self) -> Option<T> {
        self.changed = false;
        self.handler.take()
    }

    pub(super) fn finish_access(&mut self, handler: Option<T>, live: bool) -> Option<T> {
        let restore = live && !self.changed && self.handler.is_none();
        self.active = false;
        self.changed = false;
        if restore {
            self.handler = handler;
            None
        } else {
            handler
        }
    }
}
