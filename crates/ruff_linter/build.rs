use pyo3_build_config::{self, BuildFlag};
use std::{env, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=RUFF_PYTHON_HOME");
    println!("cargo:rerun-if-env-changed=PYO3_PYTHON");

    if env::var("CARGO_FEATURE_EXT_LINT").is_err() {
        return;
    }

    println!("cargo:rustc-check-cfg=cfg(Py_GIL_DISABLED)");
    if pyo3_build_config::get()
        .build_flags
        .0
        .contains(&BuildFlag::Py_GIL_DISABLED)
    {
        println!("cargo:rustc-cfg=Py_GIL_DISABLED");
    }

    if let Ok(home) = env::var("RUFF_PYTHON_HOME") {
        println!("cargo:rustc-env=RUFF_PYTHON_HOME={home}");
        return;
    }

    let python =
        env::var("PYO3_PYTHON").expect("PYO3_PYTHON must be set when `ext-lint` is enabled");
    let output = Command::new(&python)
        .args([
            "-c",
            "import sys, json; print(json.dumps({'home': sys.base_prefix}))",
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            if let Ok(text) = String::from_utf8(output.stdout) {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(home) = value
                        .get("home")
                        .and_then(|value| value.as_str())
                        .filter(|home| !home.is_empty())
                    {
                        println!("cargo:rustc-env=RUFF_PYTHON_HOME={home}");
                    }
                }
            }
        }
        _ => {
            println!("cargo:warning=ruff_linter: failed to detect Python home using `{python}`");
        }
    }
}
