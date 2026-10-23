//! Capture-shutdown driver fragments and stable stage diagnostics.

pub(super) fn exit_action(terminate: bool) -> &'static str {
    if terminate {
        r"
        system /bin/kill -TERM $child
        system /bin/kill -TERM $child
        system /bin/kill -TERM $child
        "
    } else {
        "send -- $env(PROQI_TEST_PRIMARY_Q)"
    }
}

pub(super) fn stage(code: Option<i32>) -> &'static str {
    match code {
        Some(81) => "initial Board escape was not accepted by the reducer",
        Some(82) => "Board focus movement was not accepted by the reducer",
        Some(83) => "Screenshot Inbox activation was not accepted by the reducer",
        Some(86) => "Edit entry was not accepted by the reducer",
        Some(87) => "no Edit acceptance receipt was observed before shutdown",
        Some(93) => "Screenshot Inbox did not publish its capture authority",
        Some(95) => "SQLite transaction readiness failed or reported an error",
        Some(94) => "owner control was published while capture authority was absent",
        Some(96) => "shutdown exceeded the fixture's bounded lifecycle oracle",
        Some(_) => "the child or fixture returned an unexpected status",
        None => "the fixture ended without an exit code",
    }
}

/// Own both CLI prompts and reject errors before acknowledging the write lock.
pub(super) fn sqlite_transaction() -> &'static str {
    r#"
        proc await_sqlite_prompt {} {
            global spawn_id
            expect {
                -re {(?i)(error|locked|busy)} { exit 95 }
                -exact "proqi-sql-primary> " {}
                timeout { exit 98 }
                eof { exit 99 }
            }
        }
        spawn /usr/bin/sqlite3 -list -noheader -init /dev/null -cmd ".prompt 'proqi-sql-primary> ' 'proqi-sql-cont> '" $env(PROQI_TEST_DATABASE)
        set sqlite $spawn_id
        register_watchdog_pid [exp_pid]
        await_sqlite_prompt
        send -- "BEGIN IMMEDIATE;\r"
        await_sqlite_prompt
        send -- "SELECT 'proqi-' || 'transaction-held';\r"
        expect {
            -re {(?i)(error|locked|busy)} { exit 95 }
            -exact "\r\nproqi-transaction-held\r\n" {}
            timeout { exit 98 }
            eof { exit 99 }
        }
        await_sqlite_prompt
    "#
}

#[cfg(test)]
#[path = "sqlite_tests.rs"]
mod sqlite_tests;
