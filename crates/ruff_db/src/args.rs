//! Response-file expansion shared by the Ruff and ty command-line interfaces.

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::Path;

/// Expands `@path` arguments unless the literal path, including `@`, exists.
///
/// Response files contain one argument per line and may refer to other response files.
/// All paths are relative to the current working directory. Expansion happens before
/// option parsing, so the same rules apply before and after `--`.
pub fn expand_args(args: impl Iterator<Item = OsString>) -> io::Result<Vec<OsString>> {
    let mut expanded = Vec::with_capacity(args.size_hint().0);
    for arg in args {
        if is_response_file(&arg) {
            expanded.extend(argfile::expand_args_from(
                std::iter::once(arg),
                |content, prefix| {
                    content
                        .lines()
                        .map(|line| {
                            if is_response_file(OsStr::new(line)) {
                                argfile::Argument::parse_ref(line, prefix)
                            } else {
                                argfile::Argument::PassThrough(line.into())
                            }
                        })
                        .collect()
                },
                argfile::PREFIX,
            )?);
        } else {
            expanded.push(arg);
        }
    }
    Ok(expanded)
}

#[expect(
    clippy::disallowed_methods,
    reason = "CLI arguments refer to the real filesystem and may contain non-UTF-8 paths."
)]
fn is_response_file(arg: &OsStr) -> bool {
    arg.as_encoded_bytes().starts_with(b"@") && !Path::new(arg).exists()
}
