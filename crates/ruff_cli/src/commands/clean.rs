use std::fs::{read_dir, remove_dir_all, remove_file};
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use colored::Colorize;
use path_absolutize::path_dedot;
use walkdir::WalkDir;

use ruff::fs;
use ruff::logging::LogLevel;
use ruff_cache::{cache_dir, CACHE_DIR_NAME};

/// Clear any caches in the current directory or any subdirectories.
pub(crate) fn clean(
    cache_dir_overwrite: Option<PathBuf>,
    days_old: Option<usize>,
    level: LogLevel,
) -> Result<()> {
    let mut stderr = BufWriter::new(io::stderr().lock());
    let pwd = std::env::current_dir()?;
    let path = cache_dir(cache_dir_overwrite, &pwd);

    if cachedir::is_tagged(&path)? {
        if let Some(days_old) = days_old {
            let cutoff = SystemTime::now() - Duration::from_secs(days_old as u64 * 24 * 60 * 60);
            for entry in read_dir(&path)? {
                let entry = entry?;

                let last_modified = entry.metadata()?.modified()?;
                if last_modified >= cutoff {
                    continue;
                }

                let path = entry.path();
                if level >= LogLevel::Default {
                    writeln!(
                        stderr,
                        "Removing cache at: {}",
                        fs::relativize_path(&path).bold()
                    )?;
                }
                // NOTE: we don't expect any directories here, but if there are
                // this will fail.
                remove_file(&path)?;
            }
        } else {
            if level >= LogLevel::Default {
                writeln!(
                    stderr,
                    "Removing cache at: {}",
                    fs::relativize_path(&path).bold()
                )?;
            }
            remove_dir_all(path)?;
        }
    } else if level >= LogLevel::Default {
        writeln!(
            stderr,
            "Not removing cache at: {}, not a cache directory created by Ruff",
            fs::relativize_path(&path).bold()
        )?;
    }

    // This code removes the old caches that are not based on the global caches.
    //
    // TODO: after everybody moved to the new global cache usage we can remove
    // this.
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
