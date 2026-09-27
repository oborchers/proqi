use std::path::{Path, PathBuf};

use super::{
    ExportDisposition, ExportPathError, default_export_file_name, resolve_export_path,
    sanitize_file_stem, utc_file_timestamp,
};
use crate::domain::{ThoughtName, Timestamp};

#[test]
fn relative_and_home_paths_resolve_from_their_owners() {
    let base = Path::new("/work/repo");
    let home = Path::new("/home/tester");
    assert_eq!(
        resolve_export_path("notes.txt", base, Some(home)),
        Ok(PathBuf::from("/work/repo/notes.txt"))
    );
    assert_eq!(
        resolve_export_path("./docs/./a b.txt", base, Some(home)),
        Ok(PathBuf::from("/work/repo/docs/a b.txt"))
    );
    assert_eq!(
        resolve_export_path("../up.txt", base, Some(home)),
        Ok(PathBuf::from("/work/up.txt"))
    );
    assert_eq!(
        resolve_export_path("sub/../../../../../x.txt", base, Some(home)),
        Ok(PathBuf::from("/x.txt")),
        "the root's parent is the root"
    );
    assert_eq!(
        resolve_export_path("/a/./b/../c.txt", base, None),
        Ok(PathBuf::from("/a/c.txt"))
    );
    assert_eq!(
        resolve_export_path("~/Desktop/x.md", base, Some(home)),
        Ok(PathBuf::from("/home/tester/Desktop/x.md"))
    );
    assert_eq!(
        resolve_export_path("/abs/Grüße 第二.txt", base, None),
        Ok(PathBuf::from("/abs/Grüße 第二.txt"))
    );
    assert_eq!(
        resolve_export_path("~other/x.txt", base, None),
        Ok(PathBuf::from("/work/repo/~other/x.txt"))
    );
}

#[test]
fn unusable_destinations_are_typed_errors() {
    let base = Path::new("/work");
    assert_eq!(
        resolve_export_path("  ", base, None),
        Err(ExportPathError::Empty)
    );
    assert_eq!(
        resolve_export_path("~/x.txt", base, None),
        Err(ExportPathError::HomeUnavailable)
    );
    for value in ["dir/", "~", "..", "/", "a/..", "a/.", "~/.."] {
        assert_eq!(
            resolve_export_path(value, base, Some(Path::new("/h"))),
            Err(ExportPathError::MissingFileName),
            "{value}"
        );
    }
    assert_eq!(
        resolve_export_path("x.txt", Path::new("rel"), None),
        Err(ExportPathError::RelativeBase)
    );
}

#[test]
fn default_name_prefers_a_single_thought_name_and_otherwise_uses_session_and_utc_time() {
    let at = Timestamp::from_millis(1_790_342_412_345);
    let name = ThoughtName::new("Release: plan / v2?").expect("name");
    assert_eq!(
        default_export_file_name(Some(&name), "ignored", at),
        "Release- plan - v2-.txt"
    );
    assert_eq!(
        default_export_file_name(None, "proqi", at),
        format!("proqi-{}.txt", utc_file_timestamp(at))
    );
    assert_eq!(utc_file_timestamp(at), "2026-09-25-132012");
    assert_eq!(
        utc_file_timestamp(Timestamp::from_millis(0)),
        "1970-01-01-000000"
    );
    assert_eq!(
        utc_file_timestamp(Timestamp::from_millis(951_782_400_000)),
        "2000-02-29-000000"
    );
    assert_eq!(
        utc_file_timestamp(Timestamp::from_millis(-1_000)),
        "1969-12-31-235959"
    );
}

#[test]
fn sanitizer_removes_separators_controls_and_hidden_prefixes() {
    assert_eq!(sanitize_file_stem("..hidden"), "hidden");
    assert_eq!(sanitize_file_stem("a\tb\n\nc"), "a b c");
    assert_eq!(sanitize_file_stem("x\u{7}y"), "x-y");
    assert_eq!(sanitize_file_stem(r"a\b|c<d>e"), "a-b-c-d-e");
    assert_eq!(sanitize_file_stem("   "), "thoughts");
    assert_eq!(sanitize_file_stem("..."), "thoughts");
    assert_eq!(sanitize_file_stem("👩‍💻 Grüße"), "👩‍💻 Grüße");
    let long = "é".repeat(150);
    let bounded = sanitize_file_stem(&long);
    assert!(bounded.len() <= super::EXPORT_STEM_MAX_BYTES);
    assert!(bounded.chars().all(|character| character == 'é'));
}

#[test]
fn dispositions_have_stable_spellings() {
    assert_eq!(ExportDisposition::Keep.as_str(), "keep");
    assert!(!ExportDisposition::Keep.changes_board());
    assert!(ExportDisposition::Remove.changes_board());
    assert_eq!(
        serde_json::to_string(&ExportDisposition::ReplaceWithReference).expect("json"),
        "\"replace_with_reference\""
    );
}
