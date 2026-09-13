pub(super) const SOURCE: &str = r#"use std::{ffi::OsString, os::unix::fs::symlink, path::PathBuf, str::FromStr, thread, time::{Duration, Instant}};

use proqi::{
    adapters::{
        control::LocalUpdateControlClient,
        runtime::{FileRuntimeCoordinator, SystemClock, SystemIdGenerator},
        update::{FileUpdateStateStore, HomebrewFormulaInstaller, SystemInstallDetector},
    },
    application::UpdateRestartCoordinator,
    domain::{InstanceId, StableVersion, Timestamp},
    ports::{
        environment::{Clock as _, IdGenerator as _, ProcessError, ProcessOutput, ProcessRequest, ProcessRunner},
        update::{InstallDetector as _, UPDATE_CONTROL_PROTOCOL_VERSION},
    },
};

struct InstallerProcess {
    active: PathBuf,
    target: PathBuf,
    version_output: Vec<u8>,
    installer_active: PathBuf,
    late_start_observed: PathBuf,
}

impl ProcessRunner for InstallerProcess {
    fn run(&mut self, request: ProcessRequest) -> Result<ProcessOutput, ProcessError> {
        if request.program == OsString::from("brew") {
            if request.args != ["upgrade", "--formula", "oborchers/tap/proqi"].map(OsString::from) {
                return Err(ProcessError::Io("unexpected installer request".to_owned()));
            }
            std::fs::remove_file(&self.active).map_err(|error| ProcessError::Io(error.to_string()))?;
            symlink(&self.target, &self.active).map_err(|error| ProcessError::Io(error.to_string()))?;
            std::fs::write(&self.installer_active, b"active").map_err(|error| ProcessError::Io(error.to_string()))?;
            let deadline = Instant::now() + Duration::from_secs(10);
            while !self.late_start_observed.exists() {
                if Instant::now() >= deadline {
                    return Err(ProcessError::Io("late start oracle timed out".to_owned()));
                }
                thread::sleep(Duration::from_millis(10));
            }
            return Ok(ProcessOutput { exit_code: Some(0), stdout: Vec::new(), stderr: Vec::new() });
        }
        if request.program != self.active.as_os_str()
            || request.args != ["--version"].map(OsString::from)
            || std::fs::read_link(&self.active).ok().as_ref() != Some(&self.target)
        {
            return Err(ProcessError::Io("unexpected version verification request".to_owned()));
        }
        Ok(ProcessOutput {
            exit_code: Some(0),
            stdout: self.version_output.clone(),
            stderr: Vec::new(),
        })
    }
}

fn main() {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    assert_eq!(arguments.len(), 8);
    let state = PathBuf::from(&arguments[0]);
    let old = PathBuf::from(&arguments[1]);
    let active = PathBuf::from(&arguments[2]);
    let target_binary = PathBuf::from(&arguments[3]);
    let initiating = InstanceId::from_str(arguments[4].to_str().expect("instance UTF-8")).expect("instance ID");
    let target = StableVersion::parse(arguments[5].to_str().expect("version UTF-8")).expect("target version");
    let installation = SystemInstallDetector::for_executable(old).detect().expect("installation");
    let mut ids = SystemIdGenerator;
    let registry = FileRuntimeCoordinator::new(
        state.join("runtime"),
        ids.instance_id(),
        std::env::current_dir().expect("working directory"),
        SystemClock.now(),
        env!("CARGO_PKG_VERSION"),
    )
    .expect("registry")
    .with_update_context(
        installation.identity,
        UPDATE_CONTROL_PROTOCOL_VERSION,
        None,
    );
    let update_state = FileUpdateStateStore::new(&state.join("cache")).expect("update state");
    let mut gateway = LocalUpdateControlClient::new(SystemIdGenerator);
    let mut process = InstallerProcess {
        active: active.clone(),
        target: target_binary,
        version_output: format!("proqi {target}\n").into_bytes(),
        installer_active: PathBuf::from(&arguments[6]),
        late_start_observed: PathBuf::from(&arguments[7]),
    };
    let mut installer = HomebrewFormulaInstaller::new(&mut process, active);
    let deadline = Timestamp::from_millis(SystemClock.now().as_millis().saturating_add(45_000));
    let execution = UpdateRestartCoordinator::new(&update_state, &registry, &mut gateway, &mut installer, &SystemClock)
        .execute(ids.request_id(), initiating, installation.identity, &target, deadline, &())
        .expect("coordinate update");
    println!("{}", serde_json::to_string(&execution).expect("serialize execution"));
}
"#;
