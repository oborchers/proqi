use std::cell::Cell;

use super::*;

struct AdvancingClock(Cell<Timestamp>);

impl crate::ports::environment::Clock for AdvancingClock {
    fn now(&self) -> Timestamp {
        self.0.get()
    }
}

struct SlowInstaller<'a> {
    clock: &'a AdvancingClock,
    installed: StableVersion,
}

impl UpdateInstaller for SlowInstaller<'_> {
    fn upgrade(&mut self, _: &StableVersion) -> Result<StableVersion, UpdateError> {
        self.clock.0.set(Timestamp::from_millis(1_800_000_660_000));
        Ok(self.installed.clone())
    }
}

#[test]
fn post_install_preparation_receives_a_fresh_bounded_deadline() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([92; 32]);
    let participants = participants(&mut ids, identity, 2);
    let initiating = participants[0].instance_id;
    let registry = registry(participants.clone(), participants);
    let state = State::default();
    let mut gateway = Gateway::default();
    let clock = AdvancingClock(Cell::new(Timestamp::from_millis(1_800_000_000_000)));
    let mut installer = SlowInstaller {
        clock: &clock,
        installed: version("0.2.0"),
    };

    let result =
        UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer, &clock)
            .execute(
                ids.request_id(),
                initiating,
                identity,
                &version("0.2.0"),
                Timestamp::from_millis(1_800_000_030_000),
                &(),
            )
            .expect("slow installation keeps a fresh preparation phase");

    assert_eq!(result.restart_accepted, 2);
    assert_eq!(
        gateway.prepare_deadlines[..2],
        [Timestamp::from_millis(1_800_000_030_000); 2]
    );
    assert_eq!(
        gateway.prepare_deadlines[2..],
        [Timestamp::from_millis(1_800_000_690_000); 2]
    );
}
