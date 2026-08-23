//! Operating-system clock, identifiers, paths, and process environment.

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use directories::ProjectDirs;
use uuid::Uuid;

use crate::{
    domain::{
        InstanceId, OperationId, RequestId, RevisionId, SessionId, SubmissionId, ThoughtId,
        Timestamp,
    },
    ports::environment::{AppPaths, Clock, Environment, IdGenerator, PathError, Paths},
};

/// Operating-system UTC clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis());
        Timestamp::from_millis(i64::try_from(millis).unwrap_or(i64::MAX))
    }
}

/// System `UUIDv7` generator.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemIdGenerator;

macro_rules! system_id {
    ($type:ty) => {
        loop {
            if let Ok(id) = <$type>::from_uuid(Uuid::now_v7()) {
                break id;
            }
        }
    };
}

impl IdGenerator for SystemIdGenerator {
    fn session_id(&mut self) -> SessionId {
        system_id!(SessionId)
    }
    fn thought_id(&mut self) -> ThoughtId {
        system_id!(ThoughtId)
    }
    fn revision_id(&mut self) -> RevisionId {
        system_id!(RevisionId)
    }
    fn operation_id(&mut self) -> OperationId {
        system_id!(OperationId)
    }
    fn instance_id(&mut self) -> InstanceId {
        system_id!(InstanceId)
    }
    fn request_id(&mut self) -> RequestId {
        system_id!(RequestId)
    }
    fn submission_id(&mut self) -> SubmissionId {
        system_id!(SubmissionId)
    }
}

/// Platform-native Proqi path resolver.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativePaths;

impl Paths for NativePaths {
    fn resolve(&self) -> Result<AppPaths, PathError> {
        let project = ProjectDirs::from("", "", "proqi")
            .ok_or(PathError::Unavailable("project directories"))?;
        let data_dir = project.data_local_dir().to_path_buf();
        let config_dir = project.config_dir().to_path_buf();
        let runtime_dir = project
            .runtime_dir()
            .map_or_else(|| data_dir.join("runtime"), Path::to_path_buf);
        let paths = AppPaths {
            data_dir,
            config_dir,
            runtime_dir,
        };
        for path in [&paths.data_dir, &paths.config_dir, &paths.runtime_dir] {
            if !path.is_absolute() {
                return Err(PathError::Relative(path.clone()));
            }
        }
        Ok(paths)
    }
}

/// Operating-system process environment adapter.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemEnvironment;

impl Environment for SystemEnvironment {
    fn current_directory(&self) -> Result<PathBuf, PathError> {
        std::env::current_dir().map_err(|_| PathError::Unavailable("current working directory"))
    }
}
