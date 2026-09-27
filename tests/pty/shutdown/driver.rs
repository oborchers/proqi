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
        Some(94) => "owner control was published while capture authority was absent",
        Some(96) => "shutdown exceeded the fixture's bounded lifecycle oracle",
        Some(_) => "the child or fixture returned an unexpected status",
        None => "the fixture ended without an exit code",
    }
}
