//! Shared source and installed executable contract for ordinal-bearing migrations.

use crate::ordinal_fixture::{Fixture, durable_rows, query, without_ordinals};
use std::{path::Path, process::Command};

pub(super) fn assert_cli_migrations(mut command: impl FnMut() -> Command, state: &Path) {
    for schema in [13, 14, 15] {
        let root = state.join(format!("ordinal-schema-{schema}"));
        let fixture = Fixture::seed(&root, 24);
        fixture.downgrade(schema);
        let before = durable_rows(&fixture.connection());
        let legacy = without_ordinals(&fixture.connection());
        let mut migrated = None;
        for attempt in 0..3 {
            let output = command()
                .arg("--state-dir")
                .arg(&fixture.root)
                .args(["--json", "sessions", "list"])
                .output()
                .expect("installed migration");
            let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
            assert!(
                output.status.success(),
                "schema {schema}, attempt {attempt}: {value}"
            );
            assert!(
                value["data"]["sessions"]
                    .as_array()
                    .expect("sessions")
                    .iter()
                    .any(|session| session["id"] == fixture.session.to_string())
            );
            let rows = durable_rows(&fixture.connection());
            if schema >= 14 {
                assert_eq!(rows, before);
            }
            assert_eq!(without_ordinals(&fixture.connection()), legacy);
            if let Some(expected) = &migrated {
                assert_eq!(&rows, expected);
            }
            migrated = Some(rows);
            assert_eq!(
                query(
                    &fixture.connection(),
                    "SELECT schema_version, storage_protocol FROM schema_meta"
                ),
                vec![vec![16.into(), 15.into()]]
            );
            assert_eq!(
                query(&fixture.connection(), "PRAGMA quick_check"),
                vec![vec!["ok".to_owned().into()]]
            );
        }
    }
}
