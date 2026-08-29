//! macOS screenshot directory watching and stable-file reconciliation.

#[cfg(target_os = "macos")]
mod image;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod pattern;

use crate::ports::screenshot::{
    ActiveScreenshotWatcher, ScreenshotCandidate, ScreenshotError, ScreenshotInboxConfig,
    ScreenshotWatcherFactory,
};

/// Injected system factory for the platform-specific watcher.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemScreenshotWatcherFactory;

impl ScreenshotWatcherFactory for SystemScreenshotWatcherFactory {
    fn start(
        &self,
        config: ScreenshotInboxConfig,
        terminal_host: &str,
    ) -> Result<Box<dyn ActiveScreenshotWatcher>, ScreenshotError> {
        SystemScreenshotWatcher::start(config, terminal_host)
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

    fn final_reconcile(&mut self) -> Result<Vec<ScreenshotCandidate>, ScreenshotError> {
        Self::final_reconcile(self)
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
        config.validate()?;
        #[cfg(target_os = "macos")]
        {
            macos::MacScreenshotWatcher::start(config, terminal_host).map(|inner| Self { inner })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (config, terminal_host);
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
    pub fn final_reconcile(&mut self) -> Result<Vec<ScreenshotCandidate>, ScreenshotError> {
        #[cfg(target_os = "macos")]
        {
            self.inner.final_reconcile()
        }
        #[cfg(not(target_os = "macos"))]
        Err(ScreenshotError::UnsupportedPlatform)
    }
}
