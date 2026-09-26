use std::path::{Path, PathBuf};

use super::{CompletionOutcome, MAX_CANDIDATES, complete, completion_request};
use crate::ports::export::{DirectoryEntry, DirectoryListing};

fn listing(entries: &[(&str, bool)]) -> DirectoryListing {
    DirectoryListing {
        entries: entries
            .iter()
            .map(|(name, directory)| DirectoryEntry {
                name: (*name).to_owned(),
                directory: *directory,
            })
            .collect(),
        truncated: false,
    }
}

#[test]
fn requests_list_the_folder_named_by_the_typed_prefix() {
    let base = Path::new("/work/repo");
    let home = Some(Path::new("/home/tester"));
    let cases = [
        ("no", "/work/repo", "", "no"),
        ("docs/re", "/work/repo/docs/", "docs/", "re"),
        ("/tmp/x", "/tmp/", "/tmp/", "x"),
        ("~/Desk", "/home/tester/", "~/", "Desk"),
        ("~/a b/", "/home/tester/a b/", "~/a b/", ""),
    ];
    for (text, directory, prefix, partial) in cases {
        let request = completion_request(text, base, home).expect(text);
        assert_eq!(request.directory, PathBuf::from(directory), "{text}");
        assert_eq!(request.prefix, prefix, "{text}");
        assert_eq!(request.partial, partial, "{text}");
    }
    assert!(completion_request("~/x", base, None).is_none());
}

#[test]
fn a_unique_match_completes_and_folders_gain_a_separator() {
    let request = completion_request("do", Path::new("/w"), None).expect("request");
    assert_eq!(
        complete(&request, &listing(&[("docs", true), ("notes.txt", false)])),
        CompletionOutcome::Completed {
            text: "docs/".to_owned(),
            candidates: Vec::new(),
            matches: 1,
        }
    );
    let request = completion_request("docs/n", Path::new("/w"), None).expect("request");
    assert_eq!(
        complete(&request, &listing(&[("notes.txt", false)])),
        CompletionOutcome::Completed {
            text: "docs/notes.txt".to_owned(),
            candidates: Vec::new(),
            matches: 1,
        }
    );
}

#[test]
fn shared_prefixes_extend_before_offering_choices() {
    let request = completion_request("re", Path::new("/w"), None).expect("request");
    let CompletionOutcome::Completed {
        text, candidates, ..
    } = complete(
        &request,
        &listing(&[
            ("report-a.txt", false),
            ("report-b.txt", false),
            ("zeta", false),
        ]),
    )
    else {
        panic!("extended");
    };
    assert_eq!(text, "report-");
    assert_eq!(candidates.len(), 2);

    let request = completion_request("report-", Path::new("/w"), None).expect("request");
    let CompletionOutcome::Ambiguous { candidates, .. } = complete(
        &request,
        &listing(&[("report-a.txt", false), ("report-b.txt", false)]),
    ) else {
        panic!("ambiguous");
    };
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.text.as_str())
            .collect::<Vec<_>>(),
        ["report-a.txt", "report-b.txt"]
    );
}

#[test]
fn hidden_entries_need_a_leading_dot_and_unicode_prefixes_stay_whole() {
    let entries = listing(&[(".git", true), ("Grüße.txt", false), ("Grün.txt", false)]);
    let request = completion_request("", Path::new("/w"), None).expect("request");
    let CompletionOutcome::Completed {
        text,
        candidates: visible,
        ..
    } = complete(&request, &entries)
    else {
        panic!("shared visible prefix");
    };
    assert_eq!(
        text, "Grü",
        "the hidden entry does not block the shared prefix"
    );
    assert_eq!(visible.len(), 2);
    assert!(
        visible
            .iter()
            .all(|candidate| !candidate.label.starts_with('.'))
    );
    let request = completion_request(".", Path::new("/w"), None).expect("request");
    assert!(matches!(
        complete(&request, &entries),
        CompletionOutcome::Completed { ref text, .. } if text == ".git/"
    ));
    let request = completion_request("G", Path::new("/w"), None).expect("request");
    assert!(matches!(
        complete(&request, &entries),
        CompletionOutcome::Completed { ref text, .. } if text == "Grü"
    ));
    let request = completion_request("x", Path::new("/w"), None).expect("request");
    assert_eq!(complete(&request, &entries), CompletionOutcome::NoMatch);
}

#[test]
fn matches_beyond_the_offered_rows_still_decide_the_shared_prefix() {
    let mut names = (0..100)
        .map(|index| format!("note-{index:03}.txt"))
        .collect::<Vec<_>>();
    let entries = |names: &[String]| {
        listing(
            &names
                .iter()
                .map(|name| (name.as_str(), false))
                .collect::<Vec<_>>(),
        )
    };
    let request = completion_request("note-", Path::new("/w"), None).expect("request");
    let CompletionOutcome::Completed {
        text,
        candidates,
        matches,
    } = complete(&request, &entries(&names))
    else {
        panic!("shared prefix");
    };
    assert_eq!(text, "note-0");
    assert_eq!(matches, 100);
    assert_eq!(
        candidates.len(),
        MAX_CANDIDATES,
        "only the offered rows are capped"
    );

    // A match sorted beyond the offered rows prevents a false extension.
    names.push("note-zeta.txt".to_owned());
    let CompletionOutcome::Ambiguous {
        candidates,
        matches,
    } = complete(&request, &entries(&names))
    else {
        panic!("ambiguous");
    };
    assert_eq!(matches, 101);
    assert_eq!(candidates.len(), MAX_CANDIDATES);
}

#[test]
fn a_truncated_listing_never_claims_a_unique_match_or_no_match() {
    let request = completion_request("zz", Path::new("/w"), None).expect("request");
    let mut entries = listing(&[("zz-only.txt", false)]);
    entries.truncated = true;
    assert_eq!(complete(&request, &entries), CompletionOutcome::TooLarge);
    entries.entries.clear();
    assert_eq!(complete(&request, &entries), CompletionOutcome::TooLarge);
}
