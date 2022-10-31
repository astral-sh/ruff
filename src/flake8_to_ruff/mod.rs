//! Utility to generate Ruff's pyproject.toml section from a Flake8 INI file.

use std::collections::HashMap;

use anyhow::Result;

use crate::settings::options::Options;
use crate::settings::pyproject::Pyproject;

mod parser;

pub fn convert(config: HashMap<String, HashMap<String, Option<String>>>) -> Result<Pyproject> {
    // Extract the Flake8 section.
    let flake8 = config
        .get("flake8")
        .expect("Unable to find flake8 section in INI file.");

    // Parse each supported option.
    let mut options: Options = Default::default();
    for (key, value) in flake8 {
        match key.as_str() {
            "line-length" | "line_length" => match value.clone().unwrap().parse::<usize>() {
                Ok(line_length) => options.line_length = Some(line_length),
                Err(e) => eprintln!("Unable to parse '{key}' property: {e}"),
            },
            "select" => {
                options.select = Some(parser::parse_prefix_codes(value.clone().unwrap()));
            }
            "extend-select" | "extend_select" => {
                options.extend_select = parser::parse_prefix_codes(value.clone().unwrap());
            }
            "ignore" => {
                options.ignore = parser::parse_prefix_codes(value.clone().unwrap());
            }
            "extend-ignore" | "extend_ignore" => {
                options.extend_ignore = parser::parse_prefix_codes(value.clone().unwrap());
            }
            "exclude" => {
                options.exclude = Some(parser::parse_strings(value.clone().unwrap()));
            }
            "extend-exclude" | "extend_exclude" => {
                options.extend_exclude = parser::parse_strings(value.clone().unwrap());
            }
            "per-file-ignores" | "per_file_ignores" => {
                match parser::parse_files_to_codes_mapping(value.clone().unwrap()) {
                    Ok(per_file_ignores) => options.per_file_ignores = per_file_ignores,
                    Err(e) => eprintln!("Unable to parse '{key}' property: {e}"),
                }
            }
            _ => eprintln!("Skipping unsupported property: {key}"),
        }
    }

    // Create the pyproject.toml.
    Ok(Pyproject::new(options))
}
