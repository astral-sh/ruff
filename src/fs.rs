use anyhow::Result;
use lazy_static::lazy_static;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

lazy_static! {
    // TODO(charlie): Make these configurable.
    static ref EXCLUDES: HashSet<&'static str> = vec![
        "resources/test/cpython/Lib/ctypes/test/test_numbers.py",
        "resources/test/cpython/Lib/dataclasses.py",
        "resources/test/cpython/Lib/lib2to3/tests/data/bom.py",
        "resources/test/cpython/Lib/lib2to3/tests/data/crlf.py",
        "resources/test/cpython/Lib/lib2to3/tests/data/different_encoding.py",
        "resources/test/cpython/Lib/lib2to3/tests/data/false_encoding.py",
        "resources/test/cpython/Lib/lib2to3/tests/data/py2_test_grammar.py",
        "resources/test/cpython/Lib/sqlite3/test/factory.py",
        "resources/test/cpython/Lib/sqlite3/test/hooks.py",
        "resources/test/cpython/Lib/sqlite3/test/regression.py",
        "resources/test/cpython/Lib/sqlite3/test/transactions.py",
        "resources/test/cpython/Lib/sqlite3/test/types.py",
        "resources/test/cpython/Lib/test/bad_coding2.py",
        "resources/test/cpython/Lib/test/badsyntax_3131.py",
        "resources/test/cpython/Lib/test/badsyntax_pep3120.py",
        "resources/test/cpython/Lib/test/encoded_modules/module_iso_8859_1.py",
        "resources/test/cpython/Lib/test/encoded_modules/module_koi8_r.py",
        "resources/test/cpython/Lib/test/sortperf.py",
        "resources/test/cpython/Lib/test/test_email/torture_test.py",
        "resources/test/cpython/Lib/test/test_fstring.py",
        "resources/test/cpython/Lib/test/test_genericpath.py",
        "resources/test/cpython/Lib/test/test_getopt.py",
        "resources/test/cpython/Lib/test/test_htmlparser.py",
        "resources/test/cpython/Lib/test/test_importlib/stubs.py",
        "resources/test/cpython/Lib/test/test_importlib/test_files.py",
        "resources/test/cpython/Lib/test/test_importlib/test_metadata_api.py",
        "resources/test/cpython/Lib/test/test_importlib/test_open.py",
        "resources/test/cpython/Lib/test/test_importlib/test_util.py",
        "resources/test/cpython/Lib/test/test_named_expressions.py",
        "resources/test/cpython/Lib/test/test_peg_generator/__main__.py",
        "resources/test/cpython/Lib/test/test_pipes.py",
        "resources/test/cpython/Lib/test/test_source_encoding.py",
        "resources/test/cpython/Lib/test/test_weakref.py",
        "resources/test/cpython/Lib/test/test_webbrowser.py",
        "resources/test/cpython/Lib/tkinter/__main__.py",
        "resources/test/cpython/Lib/tkinter/test/test_tkinter/test_variables.py",
        "resources/test/cpython/Modules/_decimal/libmpdec/literature/fnt.py",
        "resources/test/cpython/Modules/_decimal/tests/deccheck.py",
        "resources/test/cpython/Tools/i18n/pygettext.py",
        "resources/test/cpython/Tools/test2to3/maintest.py",
        "resources/test/cpython/Tools/test2to3/setup.py",
        "resources/test/cpython/Tools/test2to3/test/test_foo.py",
        "resources/test/cpython/Tools/test2to3/test2to3/hello.py",
    ]
    .into_iter()
    .collect();
}

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
        .filter(|entry| !EXCLUDES.contains(entry.path().to_string_lossy().deref()))
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
