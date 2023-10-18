use std::process::Command;

fn main() {
    commit_info();
    #[allow(clippy::disallowed_methods)]
    let target = std::env::var("TARGET").unwrap();
    println!("cargo:rustc-env=RUST_HOST_TARGET={target}");
}

fn commit_info() {
    let output = match Command::new("git")
        .arg("log")
        .arg("-1")
        .arg("--date=short")
        .arg("--abbrev=9")
        .arg("--format=%H %h %cd %(describe)")
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return,
    };
    let stdout = String::from_utf8(output.stdout).unwrap();
    let mut parts = stdout.split_whitespace();
    let mut next = || parts.next().unwrap();
    println!("cargo:rustc-env=RUFF_COMMIT_HASH={}", next());
    println!("cargo:rustc-env=RUFF_COMMIT_SHORT_HASH={}", next());
    println!("cargo:rustc-env=RUFF_COMMIT_DATE={}", next());

    // Describe can fail for some commits
    // https://git-scm.com/docs/pretty-formats#Documentation/pretty-formats.txt-emdescribeoptionsem
    if let Some(describe) = parts.next() {
        let mut describe_parts = describe.split('-');
        println!(
            "cargo:rustc-env=RUFF_LAST_TAG={}",
            describe_parts.next().unwrap()
        );
        // If this is the tagged commit, this component will be missing
        println!(
            "cargo:rustc-env=RUFF_LAST_TAG_DISTANCE={}",
            describe_parts.next().unwrap_or("0")
        );
    }
}
