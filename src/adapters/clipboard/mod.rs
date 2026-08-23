//! Native process clipboard with an OSC 52 write fallback.

use std::{ffi::OsString, time::Duration};

use base64::{Engine, engine::general_purpose::STANDARD};

use crate::ports::{
    clipboard::{Clipboard, ClipboardError, ClipboardWrite},
    environment::{ProcessError, ProcessRequest, ProcessRunner},
};

const TIMEOUT: Duration = Duration::from_secs(2);
const OSC52_MAX_BYTES: usize = 100_000;

/// Platform command adapter backed by a direct process runner.
pub struct PlatformClipboard<R> {
    runner: R,
    osc52: bool,
}

impl<R> PlatformClipboard<R> {
    /// Enable native access and the terminal write fallback.
    #[must_use]
    pub const fn new(runner: R) -> Self {
        Self {
            runner,
            osc52: true,
        }
    }

    /// Disable OSC 52 for terminals whose policy forbids it.
    #[must_use]
    pub const fn without_osc52(mut self) -> Self {
        self.osc52 = false;
        self
    }
}

impl<R: ProcessRunner> Clipboard for PlatformClipboard<R> {
    fn write(&mut self, content: &str) -> Result<ClipboardWrite, ClipboardError> {
        for request in write_requests(content.as_bytes().to_vec()) {
            if self
                .runner
                .run(request)
                .is_ok_and(|output| output.exit_code == Some(0))
            {
                return Ok(ClipboardWrite::Native);
            }
        }
        if !self.osc52 {
            return Err(ClipboardError::Unavailable(
                "native providers failed and OSC 52 is disabled".to_owned(),
            ));
        }
        osc52(content).map(ClipboardWrite::Osc52)
    }

    fn read(&mut self) -> Result<String, ClipboardError> {
        let mut timed_out = false;
        for request in read_requests() {
            match self.runner.run(request) {
                Ok(output) if output.exit_code == Some(0) => {
                    return String::from_utf8(output.stdout)
                        .map_err(|_| ClipboardError::InvalidText);
                }
                Err(ProcessError::TimedOut) => timed_out = true,
                Ok(_) | Err(_) => {}
            }
        }
        if timed_out {
            Err(ClipboardError::TimedOut)
        } else {
            Err(ClipboardError::Unavailable(
                "no native clipboard provider succeeded".to_owned(),
            ))
        }
    }
}

fn osc52(content: &str) -> Result<Vec<u8>, ClipboardError> {
    if content.len() > OSC52_MAX_BYTES {
        return Err(ClipboardError::TooLarge);
    }
    let encoded = STANDARD.encode(content.as_bytes());
    let mut sequence = Vec::with_capacity(encoded.len().saturating_add(8));
    sequence.extend_from_slice(b"\x1b]52;c;");
    sequence.extend_from_slice(encoded.as_bytes());
    sequence.push(0x07);
    Ok(sequence)
}

#[cfg(target_os = "macos")]
fn write_requests(input: Vec<u8>) -> Vec<ProcessRequest> {
    vec![request("/usr/bin/pbcopy", &[], Some(input))]
}

#[cfg(target_os = "macos")]
fn read_requests() -> Vec<ProcessRequest> {
    vec![request("/usr/bin/pbpaste", &[], None)]
}

#[cfg(target_os = "linux")]
fn write_requests(input: Vec<u8>) -> Vec<ProcessRequest> {
    vec![
        request(
            "wl-copy",
            &["--type", "text/plain;charset=utf-8"],
            Some(input.clone()),
        ),
        request("xclip", &["-selection", "clipboard", "-in"], Some(input)),
    ]
}

#[cfg(target_os = "linux")]
fn read_requests() -> Vec<ProcessRequest> {
    vec![
        request("wl-paste", &["--no-newline"], None),
        request("xclip", &["-selection", "clipboard", "-out"], None),
    ]
}

#[cfg(target_os = "windows")]
fn write_requests(input: Vec<u8>) -> Vec<ProcessRequest> {
    vec![request(
        "powershell.exe",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::InputEncoding = [Text.UTF8Encoding]::new($false); [Console]::In.ReadToEnd() | Set-Clipboard",
        ],
        Some(input),
    )]
}

#[cfg(target_os = "windows")]
fn read_requests() -> Vec<ProcessRequest> {
    vec![request(
        "powershell.exe",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); [Console]::Out.Write((Get-Clipboard -Raw))",
        ],
        None,
    )]
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn write_requests(_input: Vec<u8>) -> Vec<ProcessRequest> {
    Vec::new()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn read_requests() -> Vec<ProcessRequest> {
    Vec::new()
}

fn request(program: &str, args: &[&str], stdin: Option<Vec<u8>>) -> ProcessRequest {
    ProcessRequest {
        program: OsString::from(program),
        args: args.iter().map(OsString::from).collect(),
        stdin,
        timeout: TIMEOUT,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use crate::{
        adapters::memory::FakeProcessRunner,
        ports::{
            clipboard::{Clipboard, ClipboardWrite},
            environment::{ProcessError, ProcessOutput},
        },
    };

    use super::PlatformClipboard;

    #[test]
    fn native_failure_returns_an_exact_bounded_osc52_sequence() {
        let runner = FakeProcessRunner {
            results: VecDeque::from([Err(ProcessError::Io("unavailable".to_owned()))]),
            ..FakeProcessRunner::default()
        };
        let mut clipboard = PlatformClipboard::new(runner);
        assert_eq!(
            clipboard.write("Grüße\n"),
            Ok(ClipboardWrite::Osc52(
                b"\x1b]52;c;R3LDvMOfZQo=\x07".to_vec()
            ))
        );
    }

    #[test]
    fn successful_native_read_preserves_exact_text() {
        let runner = FakeProcessRunner {
            results: VecDeque::from([Ok(ProcessOutput {
                exit_code: Some(0),
                stdout: b" exact\r\n".to_vec(),
                stderr: Vec::new(),
            })]),
            ..FakeProcessRunner::default()
        };
        let mut clipboard = PlatformClipboard::new(runner);
        assert_eq!(clipboard.read().expect("clipboard"), " exact\r\n");
    }
}
