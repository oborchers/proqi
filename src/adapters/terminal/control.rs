//! Recoverable terminal ownership.

use std::{
    io::{self, Write as _, stdout},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use crossterm::{
    cursor::{Hide, SetCursorStyle, Show},
    event::{
        DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
        EnableFocusChange, EnableMouseCapture, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use super::TerminalError;
use super::host::TerminalHost;
use crate::ui::KeyboardEnhancement;

pub(super) trait TerminalControl {
    fn enter(&mut self) -> io::Result<()>;
    fn restore(&mut self) -> io::Result<()>;
}

pub(super) struct CrosstermControl {
    preference: KeyboardEnhancement,
    enabled: u8,
}

const RAW_MODE: u8 = 1 << 0;
const SCREEN_MODE: u8 = 1 << 1;
const FOCUS_MODE: u8 = 1 << 2;
const KEYBOARD_MODE: u8 = 1 << 3;

impl CrosstermControl {
    pub(super) const fn new(preference: KeyboardEnhancement) -> Self {
        Self {
            preference,
            enabled: 0,
        }
    }

    const fn has(&self, mode: u8) -> bool {
        self.enabled & mode != 0
    }

    fn enable(&mut self, mode: u8) {
        self.enabled |= mode;
    }

    fn disable(&mut self, mode: u8) {
        self.enabled &= !mode;
    }
}

impl TerminalControl for CrosstermControl {
    fn enter(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        self.enable(RAW_MODE);
        execute!(
            stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste,
            SetCursorStyle::BlinkingBlock,
            Hide
        )?;
        self.enable(SCREEN_MODE);
        if execute!(stdout(), EnableFocusChange).is_ok() {
            self.enable(FOCUS_MODE);
        }
        if self.preference == KeyboardEnhancement::Auto
            && execute!(
                stdout(),
                PushKeyboardEnhancementFlags(compatible_keyboard_flags())
            )
            .is_ok()
        {
            self.enable(KEYBOARD_MODE);
        }
        Ok(())
    }

    fn restore(&mut self) -> io::Result<()> {
        let mut first = None;
        record(
            &mut first,
            execute!(stdout(), Show, SetCursorStyle::DefaultUserShape),
        );
        if self.has(KEYBOARD_MODE) {
            record(&mut first, execute!(stdout(), PopKeyboardEnhancementFlags));
            self.disable(KEYBOARD_MODE);
        }
        record(&mut first, reset_keyboard_reporting());
        if self.has(FOCUS_MODE) {
            record(&mut first, execute!(stdout(), DisableFocusChange));
            self.disable(FOCUS_MODE);
        }
        if self.has(SCREEN_MODE) {
            record(
                &mut first,
                execute!(
                    stdout(),
                    DisableBracketedPaste,
                    DisableMouseCapture,
                    LeaveAlternateScreen
                ),
            );
            self.disable(SCREEN_MODE);
        }
        if self.has(RAW_MODE) {
            record(&mut first, disable_raw_mode());
            self.disable(RAW_MODE);
        }
        first.map_or(Ok(()), Err)
    }
}

pub(super) fn compatible_keyboard_flags() -> KeyboardEnhancementFlags {
    keyboard_flags(&TerminalHost::detect())
}

fn keyboard_flags(host: &TerminalHost) -> KeyboardEnhancementFlags {
    let mut flags = KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
        | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS;
    if host.supports_keyboard_event_types() {
        flags |= KeyboardEnhancementFlags::REPORT_EVENT_TYPES;
    }
    flags
}

pub(super) fn reset_keyboard_reporting() -> io::Result<()> {
    let mut output = stdout();
    output.write_all(b"\x1b[<u\x1b[>4;0m")?;
    output.flush()
}

fn record(first: &mut Option<io::Error>, result: io::Result<()>) {
    if let Err(error) = result
        && first.is_none()
    {
        *first = Some(error);
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

type PanicHook = dyn Fn(&std::panic::PanicHookInfo<'_>) + Send + Sync + 'static;

pub(super) struct PanicHookGuard {
    previous: Option<Arc<PanicHook>>,
}

impl PanicHookGuard {
    pub(super) fn install() -> Self {
        let owner = std::thread::current().id();
        let previous: Arc<PanicHook> = std::panic::take_hook().into();
        let chained: Arc<PanicHook> = Arc::clone(&previous);
        std::panic::set_hook(Box::new(move |information| {
            let is_owner = std::thread::current().id() == owner;
            crate::adapters::diagnostics::record(
                crate::adapters::diagnostics::SafeEvent::RuntimePanicked {
                    role: if is_owner { "owner" } else { "worker" },
                },
            );
            if is_owner {
                let _restored = emergency_restore();
            }
            if is_owner {
                chained(information);
            }
        }));
        Self {
            previous: Some(previous),
        }
    }
}

impl Drop for PanicHookGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            std::panic::set_hook(Box::new(move |information| previous(information)));
        }
    }
}

fn emergency_restore() -> io::Result<()> {
    let mut first = None;
    record(
        &mut first,
        execute!(stdout(), Show, SetCursorStyle::DefaultUserShape),
    );
    record(&mut first, execute!(stdout(), PopKeyboardEnhancementFlags));
    record(&mut first, reset_keyboard_reporting());
    record(&mut first, execute!(stdout(), DisableFocusChange));
    record(
        &mut first,
        execute!(
            stdout(),
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen
        ),
    );
    record(&mut first, disable_raw_mode());
    first.map_or(Ok(()), Err)
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
    use signal_hook::consts::{SIGINT, SIGTERM};

    let mut registrations = Vec::new();
    for signal in [SIGINT, SIGTERM] {
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

    use crossterm::event::KeyboardEnhancementFlags;

    use super::{TerminalControl, TerminalGuard, keyboard_flags};
    use crate::adapters::terminal::host::TerminalHost;

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

    #[test]
    fn incompatible_transports_omit_event_type_reporting() {
        let ordinary = keyboard_flags(&TerminalHost::from_values(
            "Apple_Terminal",
            "xterm-256color",
        ));
        assert!(ordinary.contains(KeyboardEnhancementFlags::REPORT_EVENT_TYPES));
        for (program, term) in [
            ("iTerm.app", "xterm-256color"),
            ("ghostty", "xterm-ghostty"),
            ("", "tmux-256color"),
        ] {
            let flags = keyboard_flags(&TerminalHost::from_values(program, term));
            assert!(!flags.contains(KeyboardEnhancementFlags::REPORT_EVENT_TYPES));
            assert!(flags.contains(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES));
        }
    }
}
