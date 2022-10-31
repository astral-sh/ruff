use std::collections::HashMap;

use anyhow::Result;

use ruff::settings::options::Options;
use ruff::settings::pyproject::Pyproject;

use crate::parser;

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
                options.select = Some(parser::parse_prefix_codes(value.as_ref().unwrap()));
            }
            "extend-select" | "extend_select" => {
                options.extend_select = parser::parse_prefix_codes(value.as_ref().unwrap());
            }
            "ignore" => {
                options.ignore = parser::parse_prefix_codes(value.as_ref().unwrap());
            }
            "extend-ignore" | "extend_ignore" => {
                options.extend_ignore = parser::parse_prefix_codes(value.as_ref().unwrap());
            }
            "exclude" => {
                options.exclude = Some(parser::parse_strings(value.as_ref().unwrap()));
            }
            "extend-exclude" | "extend_exclude" => {
                options.extend_exclude = parser::parse_strings(value.as_ref().unwrap());
            }
            "per-file-ignores" | "per_file_ignores" => {
                match parser::parse_files_to_codes_mapping(value.as_ref().unwrap()) {
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
