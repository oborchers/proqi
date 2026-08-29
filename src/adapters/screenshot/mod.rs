//! macOS screenshot directory watching and stable-file reconciliation.

#[cfg(target_os = "macos")]
mod image;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod pattern;

use std::sync::Arc;

use crate::ports::screenshot::{
    ActiveScreenshotWatcher, ScreenshotCancellation, ScreenshotCandidate, ScreenshotError,
    ScreenshotInboxConfig, ScreenshotWatcherFactory,
};

/// Injected system factory for the platform-specific watcher.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemScreenshotWatcherFactory;

impl ScreenshotWatcherFactory for SystemScreenshotWatcherFactory {
    fn start(
        &self,
        config: ScreenshotInboxConfig,
        terminal_host: &str,
        cancellation: Arc<dyn ScreenshotCancellation>,
    ) -> Result<Box<dyn ActiveScreenshotWatcher>, ScreenshotError> {
        SystemScreenshotWatcher::start_cancellable(config, terminal_host, cancellation)
            .map(|watcher| Box::new(watcher) as Box<dyn ActiveScreenshotWatcher>)
    }
}

/// System screenshot watcher. It is intentionally unsupported outside macOS.
pub struct SystemScreenshotWatcher {
    #[cfg(target_os = "macos")]
    inner: macos::MacScreenshotWatcher,
}

impl ActiveScreenshotWatcher for SystemScreenshotWatcher {
    fn poll(&mut self) -> Result<Vec<ScreenshotCandidate>, ScreenshotError> {
        Self::poll(self)
    }

    fn final_reconcile(
        &mut self,
        budget: std::time::Duration,
    ) -> Result<Vec<ScreenshotCandidate>, ScreenshotError> {
        Self::final_reconcile(self, budget)
    }
}

impl SystemScreenshotWatcher {
    /// Start watching before taking the activation baseline.
    ///
    /// # Errors
    ///
    /// Returns a typed platform, permission, configuration, or watcher failure.
    pub fn start(
        config: ScreenshotInboxConfig,
        terminal_host: &str,
    ) -> Result<Self, ScreenshotError> {
        Self::start_cancellable(config, terminal_host, Arc::new(NeverCancelled))
    }

    fn start_cancellable(
        config: ScreenshotInboxConfig,
        terminal_host: &str,
        cancellation: Arc<dyn ScreenshotCancellation>,
    ) -> Result<Self, ScreenshotError> {
        config.validate()?;
        #[cfg(target_os = "macos")]
        {
            macos::MacScreenshotWatcher::start(config, terminal_host, cancellation)
                .map(|inner| Self { inner })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (config, terminal_host, cancellation);
            Err(ScreenshotError::UnsupportedPlatform)
        }
    }

    /// Wait for one bounded interval and reconcile all current directory state.
    ///
    /// # Errors
    ///
    /// Returns a typed permission, watcher, or reconciliation failure.
    pub fn poll(&mut self) -> Result<Vec<ScreenshotCandidate>, ScreenshotError> {
        #[cfg(target_os = "macos")]
        {
            self.inner.poll()
        }
        #[cfg(not(target_os = "macos"))]
        Err(ScreenshotError::UnsupportedPlatform)
    }

    /// Perform one final nonblocking reconciliation before relinquishing ownership.
    ///
    /// # Errors
    ///
    /// Returns a typed permission or reconciliation failure.
    pub fn final_reconcile(
        &mut self,
        budget: std::time::Duration,
    ) -> Result<Vec<ScreenshotCandidate>, ScreenshotError> {
        #[cfg(target_os = "macos")]
        {
            self.inner.final_reconcile(budget)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = budget;
            Err(ScreenshotError::UnsupportedPlatform)
        }
    }
}

struct NeverCancelled;

impl ScreenshotCancellation for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}
