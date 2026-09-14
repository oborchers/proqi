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
            let missing = missing_replacements(
                &active,
                installation,
                target,
                expected,
                crate::adapters::control::endpoint_is_live,
            );
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
    endpoint_is_live: impl Fn(&InstanceInfo) -> bool,
) -> Vec<InstanceId> {
    expected
        .iter()
        .filter(|replacement| {
            !active.iter().any(|instance| {
                instance.session_id == replacement.session_id
                    && instance.instance_id != replacement.previous_instance_id
                    && instance.pid == replacement.previous_pid
                    && instance.version == target.to_string()
                    && instance.control_endpoint.is_some()
                    && instance.update.as_ref().is_some_and(|context| {
                        replacement_context_matches(context, replacement, installation, target)
                    })
                    && endpoint_is_live(instance)
            })
        })
        .map(|replacement| replacement.previous_instance_id)
        .collect()
}

fn replacement_context_matches(
    context: &crate::ports::runtime::UpdateInstanceContext,
    expected: &UpdateReplacementExpectation,
    installation: InstallationIdentity,
    target: &StableVersion,
) -> bool {
    context.installation_identity == installation
        && context.replacement.as_deref().is_some_and(|proof| {
            proof.operation_id == expected.operation_id
                && proof.previous_instance_id == expected.previous_instance_id
                && &proof.target_version == target
        })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{
        adapters::memory::FakeIdGenerator,
        domain::{InstallationIdentity, StableVersion, Timestamp},
        ports::{
            environment::IdGenerator as _,
            runtime::{InstanceInfo, UpdateInstanceContext, UpdateReplacementContext},
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

    struct ReplacementFixture {
        instance: InstanceInfo,
        installation: InstallationIdentity,
        target: StableVersion,
        expected: UpdateReplacementExpectation,
        old: crate::domain::InstanceId,
    }

    fn replacement_fixture() -> ReplacementFixture {
        let mut ids = FakeIdGenerator::new(1_800_000_000_000);
        let old = ids.instance_id();
        let session = ids.session_id();
        let installation = InstallationIdentity::from_digest([7; 32]);
        let target = StableVersion::parse("1.2.3").expect("target");
        let instance = InstanceInfo {
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
                replacement: Some(Box::new(UpdateReplacementContext {
                    operation_id: ids.request_id(),
                    previous_instance_id: old,
                    target_version: target.clone(),
                })),
            }),
            launch_directory: "/private".to_owned(),
            started_at: Timestamp::from_millis(1),
        };
        let expected = UpdateReplacementExpectation {
            session_id: session,
            previous_instance_id: old,
            previous_pid: 42,
            operation_id: instance
                .update
                .as_ref()
                .and_then(|context| context.replacement.as_ref())
                .map(|proof| proof.operation_id)
                .expect("replacement proof"),
        };
        ReplacementFixture {
            instance,
            installation,
            target,
            expected,
            old,
        }
    }

    fn replacement_ready(fixture: &ReplacementFixture, endpoint_live: bool) -> bool {
        missing_replacements(
            std::slice::from_ref(&fixture.instance),
            fixture.installation,
            &fixture.target,
            std::slice::from_ref(&fixture.expected),
            |_| endpoint_live,
        )
        .is_empty()
    }

    #[test]
    fn replacement_requires_live_control_same_pid_and_accepted_lineage() {
        let mut fixture = replacement_fixture();
        assert!(replacement_ready(&fixture, true));
        assert!(!replacement_ready(&fixture, false));
        fixture.instance.pid = 43;
        assert!(!replacement_ready(&fixture, true));
        fixture.instance.pid = fixture.expected.previous_pid;
        fixture
            .instance
            .update
            .as_mut()
            .expect("update context")
            .replacement = None;
        assert!(!replacement_ready(&fixture, true));
    }

    #[test]
    fn replacement_rejects_wrong_endpoint_installation_version_or_instance() {
        let mut fixture = replacement_fixture();
        fixture.instance.control_endpoint = None;
        assert!(!replacement_ready(&fixture, true));
        fixture.instance.control_endpoint = Some("/private/proqi.sock".to_owned());
        fixture
            .instance
            .update
            .as_mut()
            .expect("update context")
            .installation_identity = InstallationIdentity::from_digest([8; 32]);
        assert!(!replacement_ready(&fixture, true));
        fixture
            .instance
            .update
            .as_mut()
            .expect("update context")
            .installation_identity = fixture.installation;
        fixture.instance.version = "1.2.2".to_owned();
        assert!(!replacement_ready(&fixture, true));
        fixture.instance.version = fixture.target.to_string();
        fixture.instance.storage_protocol = 999;
        fixture.instance.control_protocol = Some(999);
        fixture
            .instance
            .update
            .as_mut()
            .expect("update context")
            .protocol = 999;
        assert!(
            replacement_ready(&fixture, true),
            "the target release may advance ephemeral protocols"
        );
        fixture.instance.instance_id = fixture.old;
        assert!(!replacement_ready(&fixture, true));
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
            previous_pid: 42,
            operation_id: ids.request_id(),
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
