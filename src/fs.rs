use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::Result;
use glob::Pattern;
use walkdir::{DirEntry, WalkDir};

fn is_not_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| (entry.depth() == 0 || !s.starts_with('.')))
        .unwrap_or(false)
}

fn is_not_excluded(entry: &DirEntry, exclude: &[Pattern]) -> bool {
    entry
        .path()
        .to_str()
        .map(|s| !exclude.iter().any(|pattern| pattern.matches(s)))
        .unwrap_or(false)
}

pub fn iter_python_files<'a>(
    path: &'a PathBuf,
    exclude: &'a [Pattern],
) -> impl Iterator<Item = DirEntry> + 'a {
    WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_entry(|entry| is_not_hidden(entry) && is_not_excluded(entry, exclude))
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().to_string_lossy().ends_with(".py"))
}

pub fn read_file(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents)?;
    Ok(contents)
}
