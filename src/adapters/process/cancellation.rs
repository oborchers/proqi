//! Shared, idempotent cancellation for adapter-owned blocking work.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, Default)]
pub(crate) struct CancellationFlag(Arc<AtomicBool>);

impl CancellationFlag {
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub(crate) fn signal(&self) -> &AtomicBool {
        &self.0
    }
}

impl crate::ports::screenshot::ScreenshotCancellation for CancellationFlag {
    fn is_cancelled(&self) -> bool {
        Self::is_cancelled(self)
    }
}

impl crate::ports::update::UpdateCancellation for CancellationFlag {
    fn is_cancelled(&self) -> bool {
        Self::is_cancelled(self)
    }
}

impl crate::ports::invocation::InvocationCancellation for CancellationFlag {
    fn is_cancelled(&self) -> bool {
        Self::is_cancelled(self)
    }
}
