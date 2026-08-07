use std::process::Command;

const BUG_DEPENDENT_CLASSIC: &str =
    "1.3.5..8...67.9.2.............3....7.6.......8...14..55316...7......8....7....6..";

fn rate_with_cli(arguments: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_sukaku-forge-rate"))
        .args(arguments)
        .arg(BUG_DEPENDENT_CLASSIC)
        .output()
        .expect("run sukaku-forge-rate");
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("CLI output must be UTF-8")
}

#[test]
fn allow_uniqueness_flag_restores_the_bug_dependent_path() {
    assert_eq!(rate_with_cli(&[]), "7.1/1.2/1.2\n");
    assert_eq!(rate_with_cli(&["--allow-uniqueness"]), "5.7/1.2/1.2\n");
}
