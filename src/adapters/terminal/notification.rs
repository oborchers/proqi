//! Best-effort, content-free terminal-host pause notifications.

use std::io;

use crate::{adapters::herdr::HerdrEnvironment, application::ScreenshotPauseReason};

use super::{external::ExternalLane, host::TerminalHost};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Route {
    Disabled,
    Herdr,
    Osc9,
}

pub(super) struct PauseNotificationRouter {
    route: Route,
}

impl PauseNotificationRouter {
    pub(super) fn new(enabled: bool, herdr: HerdrEnvironment, host: &TerminalHost) -> Self {
        let route = match herdr {
            HerdrEnvironment::Enabled if enabled => Route::Herdr,
            HerdrEnvironment::Outside if enabled && host.supports_osc9() => Route::Osc9,
            HerdrEnvironment::Outside | HerdrEnvironment::Disabled | HerdrEnvironment::Enabled => {
                Route::Disabled
            }
        };
        Self { route }
    }

    pub(super) fn notify_screenshot_pause(
        &self,
        reason: ScreenshotPauseReason,
        external: &ExternalLane,
    ) {
        match self.route {
            Route::Disabled => {}
            Route::Herdr => {
                let _queued = external.notify_screenshot_pause(reason);
            }
            Route::Osc9 => {
                let output = std::io::stdout();
                let mut writer = output.lock();
                let _written = write_osc9(&mut writer, reason);
            }
        }
    }
}

fn write_osc9(writer: &mut impl io::Write, reason: ScreenshotPauseReason) -> io::Result<()> {
    writer.write_all(&sequence(reason))?;
    writer.flush()
}

fn sequence(reason: ScreenshotPauseReason) -> Vec<u8> {
    let message = format!(
        "Proqi Screenshot Inbox paused after {}.",
        reason.description()
    );
    format!("\x1b]9;{message}\x1b\\").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingWriter;

    impl std::io::Write for FailingWriter {
        fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("injected write failure"))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn routing_is_opt_in_herdr_first_and_capability_gated() {
        let ghostty = TerminalHost::from_values("ghostty", "xterm-ghostty");
        assert_eq!(
            PauseNotificationRouter::new(false, HerdrEnvironment::Enabled, &ghostty).route,
            Route::Disabled
        );
        assert_eq!(
            PauseNotificationRouter::new(true, HerdrEnvironment::Enabled, &ghostty).route,
            Route::Herdr
        );
        assert_eq!(
            PauseNotificationRouter::new(true, HerdrEnvironment::Disabled, &ghostty).route,
            Route::Disabled
        );
        assert_eq!(
            PauseNotificationRouter::new(true, HerdrEnvironment::Outside, &ghostty).route,
            Route::Osc9
        );
    }

    #[test]
    fn unknown_and_multiplexed_standalone_hosts_have_no_route() {
        for host in [
            TerminalHost::from_values("Terminal", "xterm-256color"),
            TerminalHost::from_values("iTerm.app", "tmux-256color"),
        ] {
            assert_eq!(
                PauseNotificationRouter::new(true, HerdrEnvironment::Outside, &host).route,
                Route::Disabled
            );
        }
    }

    #[test]
    fn osc9_is_exact_and_write_failure_is_bounded() {
        let mut output = Vec::new();
        write_osc9(
            &mut output,
            ScreenshotPauseReason::Inactivity { minutes: 20 },
        )
        .expect("notification");
        assert_eq!(
            output,
            b"\x1b]9;Proqi Screenshot Inbox paused after 20 minutes without activity.\x1b\\"
        );
        assert!(
            write_osc9(
                &mut FailingWriter,
                ScreenshotPauseReason::CaptureLimit { captures: 10 },
            )
            .is_err()
        );
    }
}
