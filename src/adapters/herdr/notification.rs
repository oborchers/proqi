//! Best-effort pause notification through Herdr's outer-client bridge.

use std::{ffi::OsString, time::Duration};

use crate::{
    application::ScreenshotPauseReason,
    ports::environment::{ProcessError, ProcessRequest, ProcessRunner},
};

const NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(1);
const TITLE: &str = "Proqi Screenshot Inbox paused";

/// Whether this process is outside Herdr, managed with integration disabled, or managed normally.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HerdrEnvironment {
    Outside,
    Disabled,
    Enabled,
}

impl HerdrEnvironment {
    pub(crate) fn detect() -> Self {
        Self::from_values(
            std::env::var_os("HERDR_ENV").as_deref(),
            std::env::var_os("PROQI_DISABLE_HERDR").is_some(),
        )
    }

    pub(crate) const fn integration_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }

    fn from_values(managed: Option<&std::ffi::OsStr>, disabled: bool) -> Self {
        if managed != Some(std::ffi::OsStr::new("1")) {
            Self::Outside
        } else if disabled {
            Self::Disabled
        } else {
            Self::Enabled
        }
    }
}

/// Direct, bounded Herdr notification command owned by the Herdr adapter.
pub(crate) struct HerdrPauseNotifier<R> {
    runner: R,
    program: OsString,
    enabled: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum HerdrNotificationError {
    Process(ProcessError),
    Rejected,
}

impl<R> HerdrPauseNotifier<R> {
    #[cfg(test)]
    fn new(program: OsString, runner: R, enabled: bool) -> Self {
        Self {
            runner,
            program,
            enabled,
        }
    }
}

impl HerdrPauseNotifier<crate::adapters::process::SystemProcessRunner> {
    pub(crate) fn from_environment_with_runner(
        runner: crate::adapters::process::SystemProcessRunner,
    ) -> Self {
        Self {
            runner,
            program: OsString::from("herdr"),
            enabled: HerdrEnvironment::detect().integration_enabled(),
        }
    }
}

impl<R: ProcessRunner> HerdrPauseNotifier<R> {
    pub(crate) fn notify(
        &mut self,
        reason: ScreenshotPauseReason,
    ) -> Result<(), HerdrNotificationError> {
        if !self.enabled {
            return Ok(());
        }
        let body = format!(
            "Capture paused after {}. Return to Proqi to resume.",
            reason.description()
        );
        let output = self
            .runner
            .run(ProcessRequest {
                program: self.program.clone(),
                args: [
                    "notification",
                    "show",
                    TITLE,
                    "--body",
                    &body,
                    "--position",
                    "top-right",
                    "--sound",
                    "request",
                ]
                .map(OsString::from)
                .to_vec(),
                stdin: None,
                timeout: NOTIFICATION_TIMEOUT,
            })
            .map_err(HerdrNotificationError::Process)?;
        if output.exit_code == Some(0) {
            Ok(())
        } else {
            Err(HerdrNotificationError::Rejected)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};

    use crate::ports::environment::{ProcessOutput, ProcessRequest};

    use super::*;

    #[derive(Clone, Default)]
    struct FakeRunner {
        responses: Rc<RefCell<VecDeque<Result<ProcessOutput, ProcessError>>>>,
        requests: Rc<RefCell<Vec<ProcessRequest>>>,
    }

    impl ProcessRunner for FakeRunner {
        fn run(&mut self, request: ProcessRequest) -> Result<ProcessOutput, ProcessError> {
            self.requests.borrow_mut().push(request);
            self.responses
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| Err(ProcessError::Io("missing response".to_owned())))
        }
    }

    fn output(exit_code: Option<i32>) -> ProcessOutput {
        ProcessOutput {
            exit_code,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn managed_environment_states_do_not_confuse_disabled_herdr_with_standalone() {
        assert_eq!(
            HerdrEnvironment::from_values(None, false),
            HerdrEnvironment::Outside
        );
        assert_eq!(
            HerdrEnvironment::from_values(Some(std::ffi::OsStr::new("0")), false),
            HerdrEnvironment::Outside
        );
        assert_eq!(
            HerdrEnvironment::from_values(Some(std::ffi::OsStr::new("1")), true),
            HerdrEnvironment::Disabled
        );
        assert_eq!(
            HerdrEnvironment::from_values(Some(std::ffi::OsStr::new("1")), false),
            HerdrEnvironment::Enabled
        );
    }

    #[test]
    fn exact_content_free_command_is_bounded_and_shell_free() {
        let runner = FakeRunner {
            responses: Rc::new(RefCell::new(VecDeque::from([Ok(output(Some(0)))]))),
            requests: Rc::default(),
        };
        let requests = runner.requests.clone();
        let mut notifier = HerdrPauseNotifier::new(OsString::from("herdr-fixture"), runner, true);

        notifier
            .notify(ScreenshotPauseReason::CaptureLimit { captures: 10 })
            .expect("accepted notification");

        let request = requests.borrow_mut().pop().expect("request");
        assert_eq!(request.program, OsString::from("herdr-fixture"));
        assert_eq!(
            request.args,
            [
                "notification",
                "show",
                "Proqi Screenshot Inbox paused",
                "--body",
                "Capture paused after 10 unattended captures. Return to Proqi to resume.",
                "--position",
                "top-right",
                "--sound",
                "request",
            ]
            .map(OsString::from)
        );
        assert_eq!(request.stdin, None);
        assert_eq!(request.timeout, Duration::from_secs(1));
    }

    #[test]
    fn disabled_failure_and_rejection_remain_bounded_and_non_panicking() {
        let disabled = FakeRunner::default();
        let requests = disabled.requests.clone();
        HerdrPauseNotifier::new(OsString::from("herdr"), disabled, false)
            .notify(ScreenshotPauseReason::Inactivity { minutes: 20 })
            .expect("disabled no-op");
        assert!(requests.borrow().is_empty());

        for response in [
            Err(ProcessError::TimedOut),
            Err(ProcessError::Cancelled),
            Ok(output(Some(1))),
        ] {
            let runner = FakeRunner {
                responses: Rc::new(RefCell::new(VecDeque::from([response]))),
                requests: Rc::default(),
            };
            assert!(
                HerdrPauseNotifier::new(OsString::from("herdr"), runner, true)
                    .notify(ScreenshotPauseReason::Inactivity { minutes: 20 })
                    .is_err()
            );
        }
    }
}
