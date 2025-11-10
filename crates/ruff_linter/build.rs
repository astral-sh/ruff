use pyo3_build_config::{self, BuildFlag};
use std::env;

fn main() {
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
}
