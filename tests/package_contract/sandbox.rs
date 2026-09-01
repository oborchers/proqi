//! Run-owned filesystem isolation for installed-product verification.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// One package-contract invocation's state and working directories.
pub(super) struct PackageSandbox {
    root: tempfile::TempDir,
    state: PathBuf,
    working: PathBuf,
}

impl PackageSandbox {
    pub(super) fn create(parent: &Path) -> io::Result<Self> {
        fs::create_dir_all(parent)?;
        let root = tempfile::Builder::new()
            .prefix("contract-run-")
            .tempdir_in(parent)?;
        let state = root.path().join("state");
        let working = root.path().join("working");
        for directory in ["config", "data", "cache", "runtime"] {
            fs::create_dir_all(state.join(directory))?;
        }
        fs::create_dir(&working)?;
        fs::write(
            state.join("config/config.toml"),
            b"check_for_updates = false\n",
        )?;
        Ok(Self {
            root,
            state,
            working,
        })
    }

    pub(super) fn root(&self) -> &Path {
        self.root.path()
    }

    pub(super) fn state(&self) -> &Path {
        &self.state
    }

    pub(super) fn working(&self) -> &Path {
        &self.working
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        fs,
        path::{Path, PathBuf},
        sync::Arc,
        thread,
    };

    use proqi::{
        adapters::{memory::FakeIdGenerator, runtime::FileRuntimeCoordinator},
        domain::Timestamp,
        ports::{environment::IdGenerator as _, runtime::RuntimeCoordinator as _},
    };

    use super::PackageSandbox;

    #[test]
    fn concurrent_and_repeated_runs_never_reuse_foreign_runtime_state() {
        let temporary = tempfile::tempdir().expect("package parent");
        let parent = temporary.path().join("shared-package-parent");
        let foreign_runtime = parent.join("runtime");
        let mut ids = FakeIdGenerator::new(1_725_000_000_000);
        let foreign = FileRuntimeCoordinator::new(
            foreign_runtime.clone(),
            ids.instance_id(),
            temporary.path().to_owned(),
            Timestamp::from_millis(1),
            "foreign-version",
        )
        .expect("foreign coordinator");
        let foreign_session = ids.session_id();
        let foreign_lease = foreign
            .acquire_session(foreign_session)
            .expect("foreign live owner");
        assert_eq!(foreign_lease.info().control_protocol, None);
        let stale_coordinator = FileRuntimeCoordinator::new(
            foreign_runtime.clone(),
            ids.instance_id(),
            temporary.path().to_owned(),
            Timestamp::from_millis(2),
            "stale-version",
        )
        .expect("stale coordinator");
        let stale_lease = stale_coordinator
            .acquire_session(ids.session_id())
            .expect("stale owner fixture");
        let stale_info = stale_lease.info().clone();
        drop(stale_lease);
        fs::write(
            foreign_runtime
                .join("instances")
                .join(format!("{}.json", stale_info.instance_id)),
            serde_json::to_vec(&stale_info).expect("serialize stale metadata"),
        )
        .expect("write stale metadata");

        let parent = Arc::new(parent);
        assert_parallel_sandboxes(&parent, &foreign_runtime);
        assert_repeated_sandbox_cleanup(&parent);
        let scan = foreign.scan_runtime().expect("scan foreign owners");
        assert_eq!(scan.active.len(), 1);
        assert_eq!(scan.recovered, vec![stale_info.session_id]);
        drop(foreign_lease);
        assert!(
            foreign
                .active_instances()
                .expect("foreign cleanup")
                .is_empty()
        );
    }

    fn assert_parallel_sandboxes(parent: &Arc<PathBuf>, foreign_runtime: &Path) {
        let handles = (0..16)
            .map(|_| {
                let parent = Arc::clone(parent);
                thread::spawn(move || PackageSandbox::create(&parent).expect("concurrent sandbox"))
            })
            .collect::<Vec<_>>();
        let sandboxes = handles
            .into_iter()
            .map(|handle| handle.join().expect("sandbox thread"))
            .collect::<Vec<_>>();
        let roots = sandboxes
            .iter()
            .map(|sandbox| sandbox.root().to_owned())
            .collect::<HashSet<_>>();
        assert_eq!(roots.len(), sandboxes.len());
        for sandbox in &sandboxes {
            assert_ne!(sandbox.state().join("runtime"), foreign_runtime);
            assert!(
                sandbox
                    .state()
                    .join("runtime/instances")
                    .read_dir()
                    .is_err()
            );
        }
        drop(sandboxes);
        assert!(roots.iter().all(|root| !root.exists()));
    }

    fn assert_repeated_sandbox_cleanup(parent: &Path) {
        for _ in 0..16 {
            let sandbox = PackageSandbox::create(parent).expect("repeated sandbox");
            let root = sandbox.root().to_owned();
            drop(sandbox);
            assert!(!root.exists());
        }
    }
}
