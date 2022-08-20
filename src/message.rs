use std::fmt;
use std::path::Path;

use colored::Colorize;
use regex::Regex;
use rustpython_parser::ast::Location;
use serde::{Deserialize, Serialize};

use crate::checks::CheckKind;
use crate::fs;

#[derive(Serialize, Deserialize)]
#[serde(remote = "Location")]
struct LocationDef {
    #[serde(getter = "Location::row")]
    row: usize,
    #[serde(getter = "Location::column")]
    column: usize,
}

impl From<LocationDef> for Location {
    fn from(def: LocationDef) -> Location {
        Location::new(def.row, def.column)
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub kind: CheckKind,
    #[serde(with = "LocationDef")]
    pub location: Location,
    pub filename: String,
}

impl Message {
    pub fn is_inline_ignored(&self) -> bool {
        match fs::read_line(Path::new(&self.filename), &self.location.row()) {
            Ok(line) => {
                // https://github.com/PyCQA/flake8/blob/799c71eeb61cf26c7c176aed43e22523e2a6d991/src/flake8/defaults.py#L26
                let re = Regex::new(r"(?i)# noqa(?::\s?(?P<codes>([A-Z]+[0-9]+(?:[,\s]+)?)+))?")
                    .unwrap();
                match re.captures(&line) {
                    Some(caps) => match caps.name("codes") {
                        Some(codes) => {
                            let re = Regex::new(r"[,\s]").unwrap();
                            for code in re
                                .split(codes.as_str())
                                .map(|code| code.trim())
                                .filter(|code| !code.is_empty())
                            {
                                if code == self.kind.code().as_str() {
                                    return true;
                                }
                            }
                            false
                        }
                        None => true,
                    },
                    None => false,
                }
            }
            Err(_) => false,
        }
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}{}{}{}{}{} {} {}",
            self.filename.white().bold(),
            ":".cyan(),
            self.location.row(),
            ":".cyan(),
            self.location.column(),
            ":".cyan(),
            self.kind.code().as_str().red().bold(),
            self.kind.body()
        )
    }
}
