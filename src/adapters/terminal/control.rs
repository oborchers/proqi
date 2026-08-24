//! Recoverable terminal ownership.

use std::{
    io::{self, stdout},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use crossterm::{
    cursor::{Hide, SetCursorStyle, Show},
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use super::TerminalError;

pub(super) trait TerminalControl {
    fn enter(&mut self) -> io::Result<()>;
    fn restore(&mut self) -> io::Result<()>;
}

pub(super) struct CrosstermControl;

impl TerminalControl for CrosstermControl {
    fn enter(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        execute!(
            stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
            ),
            SetCursorStyle::SteadyBlock,
            Hide
        )
    }

    fn restore(&mut self) -> io::Result<()> {
        let screen = execute!(
            stdout(),
            Show,
            SetCursorStyle::DefaultUserShape,
            PopKeyboardEnhancementFlags,
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
        let raw = disable_raw_mode();
        screen.and(raw)
    }
}

pub(super) struct TerminalGuard<C: TerminalControl> {
    control: C,
    active: bool,
}

impl<C: TerminalControl> TerminalGuard<C> {
    pub(super) fn enter(mut control: C) -> Result<Self, TerminalError> {
        if let Err(error) = control.enter() {
            let _restored = control.restore();
            return Err(error.into());
        }
        Ok(Self {
            control,
            active: true,
        })
    }

    pub(super) fn finish(mut self) -> Result<(), TerminalError> {
        let result = self.control.restore().map_err(Into::into);
        if result.is_ok() {
            self.active = false;
        }
        result
    }
}

impl<C: TerminalControl> Drop for TerminalGuard<C> {
    fn drop(&mut self) {
        if self.active {
            let _restored = self.control.restore();
        }
    }
}

pub(super) struct TerminationGuard {
    requested: Arc<AtomicBool>,
    #[cfg(unix)]
    registrations: Vec<signal_hook::SigId>,
}

impl TerminationGuard {
    pub(super) fn register() -> io::Result<Self> {
        let requested = Arc::new(AtomicBool::new(false));
        #[cfg(unix)]
        let registrations = register_unix_signals(&requested)?;
        Ok(Self {
            requested,
            #[cfg(unix)]
            registrations,
        })
    }

    pub(super) fn requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }
}

#[cfg(unix)]
fn register_unix_signals(requested: &Arc<AtomicBool>) -> io::Result<Vec<signal_hook::SigId>> {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};

    let mut registrations = Vec::new();
    for signal in [SIGINT, SIGTERM, SIGHUP] {
        match signal_hook::flag::register(signal, Arc::clone(requested)) {
            Ok(registration) => registrations.push(registration),
            Err(error) => {
                for registration in registrations {
                    signal_hook::low_level::unregister(registration);
                }
                return Err(error);
            }
        }
    }
    Ok(registrations)
}

impl Drop for TerminationGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        for registration in self.registrations.drain(..) {
            signal_hook::low_level::unregister(registration);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        sync::{Arc, Mutex},
    };

    use super::{TerminalControl, TerminalGuard};

    #[derive(Clone)]
    struct FakeControl {
        calls: Arc<Mutex<Vec<&'static str>>>,
        fail_enter: bool,
    }

    impl TerminalControl for FakeControl {
        fn enter(&mut self) -> io::Result<()> {
            self.calls.lock().expect("calls").push("enter");
            if self.fail_enter {
                Err(io::Error::other("enter failed"))
            } else {
                Ok(())
            }
        }

        fn restore(&mut self) -> io::Result<()> {
            self.calls.lock().expect("calls").push("restore");
            Ok(())
        }
    }

    fn control(fail_enter: bool) -> (FakeControl, Arc<Mutex<Vec<&'static str>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        (
            FakeControl {
                calls: Arc::clone(&calls),
                fail_enter,
            },
            calls,
        )
    }

    #[test]
    fn normal_finish_restores_exactly_once() {
        let (control, calls) = control(false);
        TerminalGuard::enter(control)
            .expect("enter")
            .finish()
            .expect("restore");
        assert_eq!(*calls.lock().expect("calls"), ["enter", "restore"]);
    }

    #[test]
    fn setup_failure_attempts_restoration() {
        let (control, calls) = control(true);
        assert!(TerminalGuard::enter(control).is_err());
        assert_eq!(*calls.lock().expect("calls"), ["enter", "restore"]);
    }

    #[test]
    fn panic_unwind_restores_terminal() {
        let (control, calls) = control(false);
        let result = std::panic::catch_unwind(|| {
            let _guard = TerminalGuard::enter(control).expect("enter");
            panic!("test unwind");
        });
        assert!(result.is_err());
        assert_eq!(*calls.lock().expect("calls"), ["enter", "restore"]);
    }
}
