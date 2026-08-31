//! Panic-safe ownership and bounded termination for one child process tree.

use std::{process::Child, thread, time::Duration};

#[cfg(unix)]
use rustix::process::{Pid, Signal, kill_process_group};

use super::Deadline;

pub(super) struct OwnedChild {
    child: Child,
    #[cfg(unix)]
    process_group: Pid,
    armed: bool,
}

impl OwnedChild {
    pub(super) fn new(child: Child) -> Self {
        #[cfg(unix)]
        let process_group = Pid::from_child(&child);
        Self {
            child,
            #[cfg(unix)]
            process_group,
            armed: true,
        }
    }

    pub(super) fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }

    pub(super) fn disarm(&mut self) {
        self.armed = false;
    }

    pub(super) fn terminate(&mut self) -> bool {
        if !self.armed {
            return true;
        }
        self.signal_tree(false);
        if self.reap_before(Deadline::new(super::TERMINATION_GRACE)) {
            self.armed = false;
            return true;
        }
        self.signal_tree(true);
        let reaped = self.reap_before(Deadline::new(super::TERMINATION_GRACE));
        if reaped {
            self.armed = false;
        }
        reaped
    }

    fn signal_tree(&mut self, force: bool) {
        #[cfg(unix)]
        {
            let signal = if force { Signal::KILL } else { Signal::TERM };
            if kill_process_group(self.process_group, signal).is_err() {
                self.signal_child(force, signal);
            }
        }
        #[cfg(not(unix))]
        {
            let _direct = self.child.kill();
        }
    }

    #[cfg(unix)]
    fn signal_child(&mut self, force: bool, signal: Signal) {
        if force {
            let _killed = self.child.kill();
            return;
        }
        let _terminated = rustix::process::kill_process(Pid::from_child(&self.child), signal);
    }

    fn reap_before(&mut self, deadline: Deadline) -> bool {
        while deadline.remaining().is_some() {
            match self.child.try_wait() {
                Ok(Some(_)) => return true,
                Ok(None) => thread::sleep(Duration::from_millis(5)),
                Err(_) => return false,
            }
        }
        false
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _reaped = self.terminate();
    }
}
