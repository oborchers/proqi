use std::{fs, os::unix::fs::symlink};

use rusqlite::Connection;

use super::*;
use crate::{
    adapters::{memory::FakeIdGenerator, runtime::SchemaLockPolicy},
    ports::{
        store::SUPPORTED_SCHEMA_VERSION,
        update::{
            ExternalUpgradeAdoptionAuthority as _, ExternalUpgradeAuthorityError, UpdateError,
        },
    },
};

#[test]
fn active_homebrew_switch_is_revalidated_under_schema_exclusion() {
    let temporary = tempfile::tempdir().expect("installation root");
    let formula = temporary.path().join("Cellar/proqi");
    let first = formula.join("0.10.0/bin/proqi");
    let second = formula.join("0.11.0/bin/proqi");
    for binary in [&first, &second] {
        fs::create_dir_all(binary.parent().expect("binary parent")).expect("create keg");
        fs::write(binary, b"distinct executable bytes").expect("write executable");
        fs::write(
            binary
                .parent()
                .expect("bin")
                .parent()
                .expect("keg")
                .join("INSTALL_RECEIPT.json"),
            b"{}",
        )
        .expect("write receipt");
    }
    let active = temporary.path().join("opt/proqi/bin/proqi");
    fs::create_dir_all(active.parent().expect("active parent")).expect("create active directory");
    symlink(&first, &active).expect("activate first keg");
    let installation = SystemInstallDetector::for_executable(first.clone())
        .detect()
        .expect("initial active installation");
    let mut ids = FakeIdGenerator::new(1_725_100_000_000);
    let coordinator = FileRuntimeCoordinator::new(
        temporary.path().join("runtime"),
        ids.instance_id(),
        temporary.path().to_path_buf(),
        Timestamp::from_millis(1),
        "0.10.0",
    )
    .expect("runtime coordinator");
    let mut executable_identity = None;
    let mut authority = RuntimeExternalUpgradeAuthority {
        coordinator: &coordinator,
        executable: &first,
        installation: &installation,
        executable_identity: &mut executable_identity,
    };
    authority
        .establish()
        .expect("establish executable identity");
    drop(authority.acquire().expect("initial authority"));

    fs::remove_file(&active).expect("remove old active link");
    symlink(&second, &active).expect("switch active link during convergence");
    let Err(error) = authority.acquire() else {
        panic!("changed active link must fail closed");
    };
    assert!(matches!(
        error,
        ExternalUpgradeAuthorityError::Update(UpdateError::Installation(_))
    ));
    let proof = coordinator
        .acquire_schema_exclusive()
        .expect("failed revalidation releases schema exclusion");
    drop(proof);
}

#[test]
fn standalone_same_path_byte_replacement_fails_post_quiescence_authority() {
    let temporary = tempfile::tempdir().expect("installation root");
    let binary = temporary.path().join("proqi");
    fs::write(&binary, b"verified startup executable bytes").expect("write executable");
    fs::write(
        temporary.path().join("proqi-installation.json"),
        br#"{"schema_version":1,"product":"proqi","kind":"standalone_archive"}"#,
    )
    .expect("write standalone marker");
    let installation = SystemInstallDetector::for_executable(binary.clone())
        .detect()
        .expect("standalone installation");
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let coordinator = FileRuntimeCoordinator::new(
        temporary.path().join("runtime"),
        ids.instance_id(),
        temporary.path().to_path_buf(),
        Timestamp::from_millis(1),
        "0.10.0",
    )
    .expect("runtime coordinator");
    let mut executable_identity = Some(
        crate::adapters::runtime::input_recovery::ExecutableIdentity::read(&binary)
            .expect("initial executable identity"),
    );
    let mut authority = RuntimeExternalUpgradeAuthority {
        coordinator: &coordinator,
        executable: &binary,
        installation: &installation,
        executable_identity: &mut executable_identity,
    };
    authority
        .establish()
        .expect("establish executable identity");

    let replacement = temporary.path().join("proqi.replacement");
    fs::write(&replacement, b"different replacement executable bytes").expect("write replacement");
    fs::rename(&replacement, &binary).expect("atomically replace executable");

    let Err(error) = authority.acquire() else {
        panic!("same-path byte replacement must fail before cache adoption");
    };
    assert!(matches!(
        error,
        ExternalUpgradeAuthorityError::Update(UpdateError::Installation(_))
    ));
    let proof = coordinator
        .acquire_schema_exclusive()
        .expect("failed identity check releases schema exclusion");
    drop(proof);
}

#[test]
fn standalone_replacement_before_authority_establishment_fails_closed() {
    let temporary = tempfile::tempdir().expect("installation root");
    let binary = temporary.path().join("proqi");
    fs::write(&binary, b"verified startup executable bytes").expect("write executable");
    fs::write(
        temporary.path().join("proqi-installation.json"),
        br#"{"schema_version":1,"product":"proqi","kind":"standalone_archive"}"#,
    )
    .expect("write standalone marker");
    let installation = SystemInstallDetector::for_executable(binary.clone())
        .detect()
        .expect("standalone installation");
    let mut executable_identity = Some(
        crate::adapters::runtime::input_recovery::ExecutableIdentity::read(&binary)
            .expect("initial executable identity"),
    );
    let replacement = temporary.path().join("proqi.replacement");
    fs::write(&replacement, b"different replacement executable bytes").expect("write replacement");
    fs::rename(&replacement, &binary).expect("atomically replace executable");

    let mut ids = FakeIdGenerator::new(1_725_300_000_000);
    let coordinator = FileRuntimeCoordinator::new(
        temporary.path().join("runtime"),
        ids.instance_id(),
        temporary.path().to_path_buf(),
        Timestamp::from_millis(1),
        "0.10.0",
    )
    .expect("runtime coordinator");
    let mut authority = RuntimeExternalUpgradeAuthority {
        coordinator: &coordinator,
        executable: &binary,
        installation: &installation,
        executable_identity: &mut executable_identity,
    };

    let Err(error) = authority.establish() else {
        panic!("pre-establishment byte replacement must fail closed");
    };
    assert!(matches!(
        error,
        ExternalUpgradeAuthorityError::Update(UpdateError::Installation(_))
    ));
}

#[test]
fn stale_migration_contender_revalidates_after_another_process_wins() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let data = temporary.path().join("data");
    let backups = data.join("backups");
    fs::create_dir(&data).expect("data directory");
    let database = data.join("proqi.sqlite3");
    Connection::open(&database)
        .expect("legacy database")
        .execute_batch("CREATE TABLE legacy(value TEXT); INSERT INTO legacy VALUES ('keep');")
        .expect("legacy fixture");
    let refuse = StoreConfig::new(
        database.clone(),
        backups.clone(),
        MigrationMode::Refuse,
        Timestamp::from_millis(1),
    );
    assert!(matches!(
        SqliteStore::open(&refuse),
        Err(StoreError::MigrationRequired { .. })
    ));

    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let runtime = temporary.path().join("runtime");
    let winner = FileRuntimeCoordinator::new(
        runtime.clone(),
        ids.instance_id(),
        temporary.path().to_path_buf(),
        Timestamp::from_millis(1),
        "winner",
    )
    .expect("winner coordinator");
    let contender = FileRuntimeCoordinator::new(
        runtime,
        ids.instance_id(),
        temporary.path().to_path_buf(),
        Timestamp::from_millis(2),
        "contender",
    )
    .expect("contender coordinator")
    .with_schema_lock_policy(
        SchemaLockPolicy::new(
            std::time::Duration::from_millis(40),
            std::time::Duration::from_millis(2),
        )
        .expect("bounded schema policy"),
    );
    let winner_lease = winner.acquire_schema_exclusive().expect("winner lease");
    drop(
        SqliteStore::open(&StoreConfig::new(
            database.clone(),
            backups.clone(),
            MigrationMode::Allow,
            Timestamp::from_millis(2),
        ))
        .expect("winner migration"),
    );
    drop(winner_lease);
    let follower_shared = winner
        .acquire_schema_shared()
        .expect("follower shared lease");

    let (store, _shared) = finish_required_migration(
        &contender,
        database.clone(),
        backups.clone(),
        &refuse,
        Timestamp::from_millis(3),
    )
    .expect("stale contender revalidates");
    drop(follower_shared);
    store.quick_check().expect("migrated integrity");
    let connection = Connection::open(database).expect("verify database");
    let schema: u32 = connection
        .query_row("SELECT schema_version FROM schema_meta", [], |row| {
            row.get(0)
        })
        .expect("schema version");
    let legacy: String = connection
        .query_row("SELECT value FROM legacy", [], |row| row.get(0))
        .expect("legacy value");
    assert_eq!(schema, SUPPORTED_SCHEMA_VERSION);
    assert_eq!(legacy, "keep");
    assert_eq!(fs::read_dir(backups).expect("backups").count(), 1);
}
