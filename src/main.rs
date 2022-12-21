#![allow(
    clippy::collapsible_else_if,
    clippy::collapsible_if,
    clippy::implicit_hasher,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::similar_names,
    clippy::too_many_lines
)]

use std::process::ExitCode;

use cfg_if::cfg_if;
use colored::Colorize;

cfg_if! {
    if #[cfg(not(target_family = "wasm"))] {
        mod main_native;
        use main_native::inner_main;
    } else {
        use anyhow::Result;

        #[allow(clippy::unnecessary_wraps)]
        fn inner_main() -> Result<ExitCode> {
            Ok(ExitCode::FAILURE)
        }
    }
}

fn main() -> ExitCode {
    match inner_main() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{} {err:?}", "error".red().bold());
            ExitCode::FAILURE
        }
    }
}
