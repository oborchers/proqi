use super::{enforce_support_floor, parse_dependencies, render_config};

#[test]
fn derived_dependencies_receive_the_declared_support_floor() {
    let derived = parse_dependencies("shlibs:Depends=libc6 (>= 2.34), libgcc-s1 (>= 4.2)\n")
        .expect("dependencies");
    assert_eq!(
        enforce_support_floor(&derived).expect("policy"),
        ["libc6 (>= 2.35)", "libgcc-s1 (>= 4.2)"]
    );
}

#[test]
fn package_config_has_exact_paths_and_no_maintainer_scripts() {
    let config = render_config(
        "0.1.2",
        &[
            "libc6 (>= 2.35)".to_owned(),
            "libgcc-s1 (>= 4.2)".to_owned(),
        ],
    )
    .expect("config");
    assert!(config.contains("dst: /usr/bin/proqi"));
    assert!(config.contains("dst: /usr/share/doc/proqi/copyright"));
    assert!(!config.contains("scripts:"));
}
