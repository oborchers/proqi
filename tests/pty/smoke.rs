use std::process::Command;

#[test]
fn release_entrypoint_can_start_without_workspace_state() {
    let output = Command::new(env!("CARGO_BIN_EXE_proqi"))
        .arg("--version")
        .output()
        .expect("run proqi binary");

    assert!(output.status.success());
}
