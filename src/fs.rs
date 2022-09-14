use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::Result;
use log::debug;
use regex::Regex;
use walkdir::{DirEntry, WalkDir};

fn is_excluded(entry: &DirEntry, exclude: &[Regex]) -> bool {
    entry
        .path()
        .to_str()
        .map(|path| exclude.iter().any(|pattern| pattern.is_match(path)))
        .unwrap_or(true)
}

fn is_included(entry: &DirEntry) -> bool {
    let path = entry.path().to_string_lossy();
    path.ends_with(".py") || path.ends_with(".pyi")
}

pub fn iter_python_files<'a>(
    path: &'a PathBuf,
    exclude: &'a [Regex],
) -> impl Iterator<Item = DirEntry> + 'a {
    WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_entry(|entry| {
            if is_excluded(entry, exclude) {
                debug!("Ignored path: {}", entry.path().to_string_lossy());
                false
            } else {
                true
            }
        })
        .filter_map(|entry| entry.ok())
        .filter(is_included)
}

pub fn read_file(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents)?;
    Ok(contents)
}
