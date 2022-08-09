use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use anyhow::Result;
use rustpython_parser::ast::Suite;
use rustpython_parser::parser;

pub fn parse(path: &Path) -> Result<Suite> {
    let file = File::open(path)?;
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents)?;
    parser::parse_program(&contents).map_err(|e| e.into())
}
