//! Repository-wide serialization for the expensive final local gate.

use std::{
    fs::{File, OpenOptions},
    io::{Read as _, Seek as _, SeekFrom, Write as _},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use fs4::{FileExt, TryLockError};
use serde_json::json;

const RETRY_INTERVAL: Duration = Duration::from_secs(1);
const REPORT_INTERVAL: Duration = Duration::from_secs(30);

pub(super) struct FinalGateLease {
    file: File,
}

impl Drop for FinalGateLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

pub(super) fn acquire(root: &Path) -> Result<FinalGateLease, String> {
    let path = lock_path(root)?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|error| format!("open final-gate lock: {error}"))?;
    let waiting_since = Instant::now();
    let mut last_report = None;
    loop {
        match FileExt::try_lock(&file) {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) => {
                report_wait(&mut file, waiting_since, &mut last_report)?;
                thread::sleep(RETRY_INTERVAL);
            }
            Err(TryLockError::Error(error)) => {
                return Err(format!("acquire final-gate lock: {error}"));
            }
        }
    }
    write_owner(&mut file, root)?;
    println!(
        "final gate lock acquired after {:.1}s",
        waiting_since.elapsed().as_secs_f64()
    );
    Ok(FinalGateLease { file })
}

fn lock_path(root: &Path) -> Result<PathBuf, String> {
    let common = command_output(root, ["rev-parse", "--git-common-dir"])?;
    let common = root
        .join(common.trim())
        .canonicalize()
        .map_err(|error| format!("canonicalize Git common directory: {error}"))?;
    Ok(common.join("proqi-final-gate.lock"))
}

fn report_wait(
    file: &mut File,
    waiting_since: Instant,
    last_report: &mut Option<Instant>,
) -> Result<(), String> {
    if last_report.is_some_and(|last| last.elapsed() < REPORT_INTERVAL) {
        return Ok(());
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|error| format!("seek final-gate owner: {error}"))?;
    let mut owner = String::new();
    file.read_to_string(&mut owner)
        .map_err(|error| format!("read final-gate owner: {error}"))?;
    println!(
        "waiting {:.1}s for final gate owner {}",
        waiting_since.elapsed().as_secs_f64(),
        owner.trim()
    );
    *last_report = Some(Instant::now());
    Ok(())
}

fn write_owner(file: &mut File, root: &Path) -> Result<(), String> {
    let branch = command_output(root, ["branch", "--show-current"])?;
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("read system time: {error}"))?
        .as_secs();
    let record = json!({
        "schema_version": 1,
        "pid": std::process::id(),
        "branch": if branch.trim().is_empty() { "detached" } else { branch.trim() },
        "started_unix": started,
    });
    file.set_len(0)
        .map_err(|error| format!("truncate final-gate owner: {error}"))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| format!("seek final-gate owner: {error}"))?;
    writeln!(file, "{record}").map_err(|error| format!("write final-gate owner: {error}"))?;
    file.flush()
        .map_err(|error| format!("flush final-gate owner: {error}"))
}

fn command_output<const N: usize>(root: &Path, arguments: [&str; N]) -> Result<String, String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| format!("start git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| format!("git output is not UTF-8: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{acquire, lock_path};
    use fs4::{FileExt, TryLockError};
    use std::{fs, fs::OpenOptions, process::Command};

    #[test]
    fn worktrees_share_one_lock_identity() {
        let temporary = tempfile::tempdir().expect("temporary root");
        let repository = temporary.path().join("repository");
        let sibling = temporary.path().join("sibling-worktree");
        fs::create_dir(&repository).expect("repository");
        run(&repository, ["init", "-q"]);
        run(&repository, ["config", "user.name", "Test"]);
        run(
            &repository,
            ["config", "user.email", "test@example.invalid"],
        );
        fs::write(repository.join("tracked"), "one\n").expect("tracked file");
        run(&repository, ["add", "tracked"]);
        run(&repository, ["commit", "-qm", "initial"]);
        run(
            &repository,
            [
                "worktree",
                "add",
                "-q",
                sibling.to_str().expect("UTF-8 path"),
            ],
        );
        assert_eq!(
            lock_path(&repository).expect("main lock"),
            lock_path(&sibling).expect("worktree lock")
        );
        run(
            &repository,
            [
                "worktree",
                "remove",
                "--force",
                sibling.to_str().expect("UTF-8 path"),
            ],
        );
    }

    #[test]
    fn dropping_the_lease_releases_the_operating_system_lock() {
        let temporary = tempfile::tempdir().expect("repository root");
        initialize_repository(temporary.path());
        let lease = acquire(temporary.path()).expect("first lease");
        let contender = OpenOptions::new()
            .read(true)
            .write(true)
            .open(lock_path(temporary.path()).expect("lock path"))
            .expect("contender");
        assert!(matches!(
            FileExt::try_lock(&contender),
            Err(TryLockError::WouldBlock)
        ));
        drop(lease);
        FileExt::try_lock(&contender).expect("released lock");
        FileExt::unlock(&contender).expect("unlock contender");
    }

    fn initialize_repository(root: &std::path::Path) {
        run(root, ["init", "-q", "-b", "main"]);
        run(root, ["config", "user.name", "Test"]);
        run(root, ["config", "user.email", "test@example.invalid"]);
        fs::write(root.join("tracked"), "one\n").expect("tracked file");
        run(root, ["add", "tracked"]);
        run(root, ["commit", "-qm", "initial"]);
    }

    fn run<const N: usize>(root: &std::path::Path, arguments: [&str; N]) {
        assert!(
            Command::new("git")
                .args(arguments)
                .current_dir(root)
                .status()
                .expect("git")
                .success()
        );
    }
}
