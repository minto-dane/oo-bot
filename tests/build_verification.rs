use std::process::Command;

#[test]
fn hardening_verify_script_gracefully_handles_missing_binary() {
    let status = Command::new("./scripts/verify_hardening.sh")
        .arg("target/release/does-not-exist")
        .arg("stable")
        .status()
        .expect("run verify script");

    assert!(status.success());
}
