//! Per-store, bounded test rendezvous at actual OS lock acquisition/contention.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, SyncSender, sync_channel},
};
use std::time::Duration;

const RENDEZVOUS_LIMIT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stage {
    Acquired,
    Contended,
}

#[derive(Debug)]
pub(super) struct LockProbe {
    stage: Stage,
    visited: AtomicBool,
    entered: SyncSender<()>,
    resume: Mutex<Receiver<()>>,
}

pub(super) struct Rendezvous {
    entered: Receiver<()>,
    resume: Option<SyncSender<()>>,
}

impl Rendezvous {
    pub(super) fn install(store: &mut super::FileUpdateStateStore, stage: Stage) -> Self {
        let (entered_tx, entered) = sync_channel(1);
        let (resume, resume_rx) = sync_channel(1);
        store.lock_probe = Some(Arc::new(LockProbe {
            stage,
            visited: AtomicBool::new(false),
            entered: entered_tx,
            resume: Mutex::new(resume_rx),
        }));
        Self {
            entered,
            resume: Some(resume),
        }
    }

    pub(super) fn reached(&self) {
        self.entered
            .recv_timeout(RENDEZVOUS_LIMIT)
            .expect("OS lock checkpoint");
    }

    pub(super) fn release(mut self) {
        self.resume
            .take()
            .expect("checkpoint sender")
            .send(())
            .expect("release checkpoint");
    }
}

// Dropping the sender unblocks a waiting worker if the test panics.
pub(super) fn observe(probe: Option<&LockProbe>, stage: Stage) {
    let Some(probe) = probe else { return };
    if probe.stage != stage || probe.visited.swap(true, Ordering::AcqRel) {
        return;
    }
    probe.entered.send(()).expect("report OS lock checkpoint");
    probe
        .resume
        .lock()
        .expect("checkpoint receiver")
        .recv_timeout(RENDEZVOUS_LIMIT)
        .expect("resume OS lock checkpoint");
}
