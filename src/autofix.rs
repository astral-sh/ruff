use std::path::Path;

use regex::Regex;

use crate::checks::CheckKind;
use crate::message::Message;

fn break_up_import(line: &str) -> String {
    return line.to_string();
}

fn multiline_import(line: &str) -> bool {
    return (line.contains('(') && !line.contains(')')) || line.trim_end().ends_with('\\');
}

fn full_name(name: &str, sep: &str, parent: &Option<String>) -> String {
    match parent {
        None => name.to_string(),
        Some(parent) => format!("{}{}{}", parent, sep, name).to_string(),
    }
}

fn _filter_imports(imports: &[&str], parent: &Option<String>, unused_module: &str) -> Vec<String> {
    let sep = match parent {
        None => ".",
        Some(parent) => {
            if parent.chars().last().map(|c| c == '.').unwrap_or_default() {
                ""
            } else {
                "."
            }
        }
    };
    let mut filtered_imports: Vec<String> = vec![];
    for name in imports {
        println!("{}", name);
        println!("{}", full_name(name, sep, &parent));
        if !unused_module.contains(&full_name(name, sep, &parent)) {
            filtered_imports.push(name.to_string());
        }
    }
    return filtered_imports;
}

fn filter_from_import(line: &str, unused_module: &str) -> Option<String> {
    // Collect indentation and imports.
    let re = Regex::new(r"\bimport\b").unwrap();
    let mut split = re.splitn(line, 2);
    let first = split.next();
    let second = split.next();
    let indentation = if second.is_some() { first.unwrap() } else { "" };
    let imports = if second.is_some() {
        second.unwrap()
    } else {
        first.unwrap()
    };

    println!("indentation: {}", indentation);
    println!("imports: {}", imports);
    // Collect base module.
    let re = Regex::new(r"\bfrom\s+([^ ]+)").unwrap();
    let base_module = re
        .captures(indentation)
        .map(|capture| capture[1].to_string());

    // Collect the list of imports.
    let re = Regex::new(r"\s*,\s*").unwrap();
    let mut imports: Vec<&str> = re.split(imports.trim()).collect();

    let filtered_imports = _filter_imports(&imports, &base_module, unused_module);
    println!("filtered_imports: {:?}", filtered_imports);
    println!("base_module: {:?}", base_module);
    println!("unused_module: {:?}", unused_module);
    if filtered_imports.is_empty() {
        None
    } else {
        Some(format!(
            "{}{}{}",
            indentation,
            filtered_imports.join(", "),
            get_line_ending(line)
        ))
    }
}

fn filter_unused_import(line: &str, unused_module: &str) -> Option<String> {
    if line.trim_start().starts_with('>') {
        return Some(line.to_string());
    }

    if multiline_import(line) {
        return Some(line.to_string());
    }

    let is_from_import = line.trim_start().starts_with("from");
    if line.contains(',') && !is_from_import {
        return Some(break_up_import(line));
    }

    if line.contains(',') {
        filter_from_import(line, unused_module)
    } else {
        None
    }
}

fn get_indentation(line: &str) -> &str {
    if line.trim().is_empty() {
        ""
    } else {
        let non_whitespace_index = line.len() - line.trim().len();
        &line[0..non_whitespace_index]
    }
}

fn get_line_ending(line: &str) -> &str {
    let non_whitespace_index = line.trim().len() - line.len();
    if non_whitespace_index == 0 {
        ""
    } else {
        &line[non_whitespace_index..]
    }
}

fn extract_package_name(line: &str) -> Option<&str> {
    if !(line.trim_start().starts_with("from") || line.trim_start().starts_with("import")) {
        return None;
    }

    let word = line.split_whitespace().skip(1).next().unwrap();
    let package = word.split('.').next().unwrap();
    return Some(package);
}

pub fn autofix(path: &Path, contents: &str, messages: &[Message]) {
    let mut fixed_lines: Vec<String> = vec![];
    let mut previous_line: Option<&str> = None;
    for (line_number, line) in contents.lines().enumerate() {
        let mut result: Option<String> = Some(line.to_string());
        for message in messages {
            if message.location.row() == line_number + 1 {
                match &message.kind {
                    CheckKind::UnusedImport(module_name) => {
                        result = filter_unused_import(line, module_name);
                    }
                    _ => {}
                }
            }
        }

        if let Some(fixed_line) = result {
            fixed_lines.push(fixed_line);
        }
        previous_line = Some(line);
    }

    println!("{}", fixed_lines.join("\n"));
}
