use std::process::Command;

fn amdtop() -> Command {
    Command::new(env!("CARGO_BIN_EXE_amdtop"))
}

#[test]
fn version_reports_package_name_and_version() {
    let output = amdtop()
        .arg("--version")
        .output()
        .expect("failed to run amdtop --version");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("amdtop {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn help_describes_the_amdtop_command() {
    let output = amdtop()
        .arg("--help")
        .output()
        .expect("failed to run amdtop --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with(&format!("amdtop {}\n", env!("CARGO_PKG_VERSION"))));
    assert!(stdout.contains("Usage: amdtop [OPTIONS]"));
    assert!(stdout.contains("--version"));
    assert!(output.stderr.is_empty());
}

#[test]
fn unknown_options_fail_without_initializing_the_tui() {
    let output = amdtop()
        .arg("--not-an-option")
        .output()
        .expect("failed to run amdtop with an invalid option");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown option: --not-an-option"));
}
