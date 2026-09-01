//! Bounded verification of peer replacement after an in-app update.

use std::{thread, time::Duration};

use crate::{
    domain::{InstallationIdentity, InstanceId, StableVersion},
    ports::{
        runtime::InstanceInfo,
        update::{
            UpdateCancellation, UpdateError, UpdateInstanceRegistry, UpdateReplacementExpectation,
        },
    },
};

use super::FileRuntimeCoordinator;

const REPLACEMENT_POLL_INTERVAL: Duration = Duration::from_millis(10);

impl UpdateInstanceRegistry for FileRuntimeCoordinator {
    fn active_instances(&self) -> Result<Vec<InstanceInfo>, UpdateError> {
        crate::ports::runtime::RuntimeCoordinator::active_instances(self)
            .map_err(|error| UpdateError::Coordination(error.to_string()))
    }

    fn wait_for_replacements(
        &self,
        installation: InstallationIdentity,
        target: &StableVersion,
        expected: &[UpdateReplacementExpectation],
        timeout: Duration,
        cancellation: &dyn UpdateCancellation,
    ) -> Result<Vec<InstanceId>, UpdateError> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let active = UpdateInstanceRegistry::active_instances(self)?;
            let missing = missing_replacements(&active, installation, target, expected);
            if missing.is_empty()
                || cancellation.is_cancelled()
                || std::time::Instant::now() >= deadline
            {
                return Ok(missing);
            }
            thread::sleep(
                REPLACEMENT_POLL_INTERVAL
                    .min(deadline.saturating_duration_since(std::time::Instant::now())),
            );
        }
    }
}

fn missing_replacements(
    active: &[InstanceInfo],
    installation: InstallationIdentity,
    target: &StableVersion,
    expected: &[UpdateReplacementExpectation],
) -> Vec<InstanceId> {
    expected
        .iter()
        .filter(|replacement| {
            !active.iter().any(|instance| {
                instance.session_id == replacement.session_id
                    && instance.instance_id != replacement.previous_instance_id
                    && instance.version == target.to_string()
                    && instance.control_endpoint.is_some()
                    && instance
                        .update
                        .as_ref()
                        .is_some_and(|context| context.installation_identity == installation)
            })
        })
        .map(|replacement| replacement.previous_instance_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{
        adapters::memory::FakeIdGenerator,
        domain::{InstallationIdentity, StableVersion, Timestamp},
        ports::{
            environment::IdGenerator as _,
            runtime::{InstanceInfo, UpdateInstanceContext},
            store::STORAGE_PROTOCOL_VERSION,
            update::{
                UPDATE_CONTROL_PROTOCOL_VERSION, UpdateCancellation, UpdateInstanceRegistry as _,
                UpdateReplacementExpectation,
            },
        },
    };

    use super::missing_replacements;

    struct AlwaysCancelled;

    impl UpdateCancellation for AlwaysCancelled {
        fn is_cancelled(&self) -> bool {
            true
        }
    }

    #[test]
    fn replacement_requires_same_session_new_instance_exact_target_and_readiness() {
        let mut ids = FakeIdGenerator::new(1_800_000_000_000);
        let old = ids.instance_id();
        let session = ids.session_id();
        let installation = InstallationIdentity::from_digest([7; 32]);
        let target = StableVersion::parse("1.2.3").expect("target");
        let mut instance = InstanceInfo {
            instance_id: ids.instance_id(),
            session_id: session,
            pid: 42,
            version: target.to_string(),
            storage_protocol: STORAGE_PROTOCOL_VERSION,
            control_protocol: Some(crate::ports::control::CONTROL_PROTOCOL_VERSION),
            control_endpoint: Some("/private/proqi.sock".to_owned()),
            update: Some(UpdateInstanceContext {
                installation_identity: installation,
                protocol: UPDATE_CONTROL_PROTOCOL_VERSION,
            }),
            launch_directory: "/private".to_owned(),
            started_at: Timestamp::from_millis(1),
        };
        let expected = [UpdateReplacementExpectation {
            session_id: session,
            previous_instance_id: old,
        }];
        assert!(
            missing_replacements(&[instance.clone()], installation, &target, &expected).is_empty()
        );
        instance.control_endpoint = None;
        assert_eq!(
            missing_replacements(&[instance.clone()], installation, &target, &expected),
            [old]
        );
        instance.control_endpoint = Some("/private/proqi.sock".to_owned());
        instance
            .update
            .as_mut()
            .expect("update context")
            .installation_identity = InstallationIdentity::from_digest([8; 32]);
        assert_eq!(
            missing_replacements(&[instance.clone()], installation, &target, &expected),
            [old]
        );
        instance
            .update
            .as_mut()
            .expect("update context")
            .installation_identity = installation;
        instance.version = "1.2.2".to_owned();
        assert_eq!(
            missing_replacements(&[instance.clone()], installation, &target, &expected),
            [old]
        );
        instance.version = target.to_string();
        instance.storage_protocol = 999;
        instance.control_protocol = Some(999);
        instance.update.as_mut().expect("update context").protocol = 999;
        assert!(
            missing_replacements(&[instance.clone()], installation, &target, &expected).is_empty(),
            "the target release may advance ephemeral protocols"
        );
        instance.instance_id = old;
        assert_eq!(
            missing_replacements(&[instance], installation, &target, &expected),
            [old]
        );
    }

    #[test]
    fn cancellation_returns_missing_replacements_without_entering_the_wait() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let mut ids = FakeIdGenerator::new(1_800_000_000_000);
        let coordinator = super::FileRuntimeCoordinator::new(
            temporary.path().join("runtime"),
            ids.instance_id(),
            temporary.path().to_path_buf(),
            Timestamp::from_millis(1),
            "1.2.3",
        )
        .expect("runtime coordinator");
        let previous = ids.instance_id();
        let expected = [UpdateReplacementExpectation {
            session_id: ids.session_id(),
            previous_instance_id: previous,
        }];

        let missing = coordinator
            .wait_for_replacements(
                InstallationIdentity::from_digest([7; 32]),
                &StableVersion::parse("1.2.3").expect("target"),
                &expected,
                Duration::from_secs(60),
                &AlwaysCancelled,
            )
            .expect("cancelled wait");

        assert_eq!(missing, [previous]);
    }
}
