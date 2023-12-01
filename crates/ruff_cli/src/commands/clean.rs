use std::fs::remove_dir_all;
use std::io::{self, BufWriter, Write};

use anyhow::Result;
use colored::Colorize;
use path_absolutize::path_dedot;
use walkdir::WalkDir;

use ruff_cache::CACHE_DIR_NAME;
use ruff_linter::fs;
use ruff_linter::logging::LogLevel;

/// Clear any caches in the current directory or any subdirectories.
pub(crate) fn clean(level: LogLevel) -> Result<()> {
    let mut stderr = BufWriter::new(io::stderr().lock());
    for entry in WalkDir::new(&*path_dedot::CWD)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_dir())
    {
        let cache = entry.path().join(CACHE_DIR_NAME);
        if cache.is_dir() {
            if level >= LogLevel::Default {
                writeln!(
                    stderr,
                    "Removing cache at: {}",
                    fs::relativize_path(&cache).bold()
                )?;
            }
            remove_dir_all(&cache)?;
        }
    }
    Ok(())
}
