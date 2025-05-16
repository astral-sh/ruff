fn main() {
    // Allow e.g. `#[cfg(test_environment="ntfs")]` to be used to select specific tests to run on different environments,
    // when the environment cannot be easily identified with standard cfg flags
    //
    // This is currently used to validate special handling of EXE001 & EXE002 on mounted windows filesystems
    //
    // Set `RUFF_TEST_ENVIRONMENT` before running tests, to specify the execution environment
    // Format is a list of values, separated using the OS-specific path separator (`:` on WSL/*nix, ";" on Windows)
    // e.g. `RUFF_TEST_ENVIRONMENT="ntfs" cargo insta ...`
    println!("cargo::rustc-check-cfg=cfg(test_environment, values(\"ntfs\"))");
    if let Some(list_of_values) = option_env!("RUFF_TEST_ENVIRONMENT") {
        for value in std::env::split_paths(list_of_values) {
            println!(
                "cargo::rustc-cfg=test_environment=\"{}\"",
                value.to_string_lossy()
            );
        }
    }
}
