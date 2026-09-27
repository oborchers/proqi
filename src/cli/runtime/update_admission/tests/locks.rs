use std::{
    sync::{Arc, Barrier},
    time::Duration,
};

use crate::{
    adapters::update::FileUpdateStateStore,
    domain::InstallationIdentity,
    ports::update::{UpdateLockKind, UpdateStateStore as _},
};

use super::super::wait_for_replacement_admission;

#[test]
fn exact_replacement_waits_for_exclusive_convergence_handoff() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let state = FileUpdateStateStore::new(&temporary.path().join("cache")).expect("update state");
    let installation = InstallationIdentity::from_digest([91; 32]);
    let convergence = state
        .try_lock(installation, UpdateLockKind::Convergence)
        .expect("convergence lock")
        .expect("exclusive convergence owner");
    let barrier = Arc::new(Barrier::new(2));
    let release_barrier = Arc::clone(&barrier);
    let release = std::thread::spawn(move || {
        release_barrier.wait();
        std::thread::sleep(Duration::from_millis(25));
        drop(convergence);
    });
    barrier.wait();

    let admission =
        wait_for_replacement_admission(&state, installation, None, Duration::from_secs(1))
            .expect("exact replacement waits for lock handoff");

    drop(admission);
    release.join().expect("convergence release");
}
