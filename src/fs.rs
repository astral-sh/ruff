use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::Result;
use walkdir::{DirEntry, WalkDir};

fn is_not_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| entry.depth() == 0 || !s.starts_with('.'))
        .unwrap_or(false)
}

pub fn iter_python_files(path: &PathBuf) -> impl Iterator<Item = DirEntry> {
    WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_entry(is_not_hidden)
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().to_string_lossy().ends_with(".py"))
}

pub fn read_line(path: &Path, row: &usize) -> Result<String> {
    let file = File::open(path)?;
    let buf_reader = BufReader::new(file);
    buf_reader
        .lines()
        .nth(*row - 1)
        .unwrap()
        .map_err(|e| e.into())
}

pub fn read_file(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents)?;
    Ok(contents)
}
