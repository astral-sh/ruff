use std::ffi::{OsStr, OsString};
use std::io::Write;

use anyhow::Result;
use clap::CommandFactory;
use colored::Colorize;

use crate::args::Args;
use crate::output_ui::{write_text_block, write_three_col_block};

#[derive(Default)]
struct Section {
    title: String,
    lines: Vec<String>,
}

fn parse_section_title(line: &str) -> Option<String> {
    if !line.ends_with(':') {
        return None;
    }
    let title = line.trim_end_matches(':').trim();
    if title.is_empty() || title.starts_with(' ') {
        return None;
    }
    Some(title.to_string())
}

fn split_columns(line: &str) -> Option<(String, String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let first_split = trimmed.find("  ")?;
    let left = trimmed[..first_split].trim().to_string();
    let mut rest = trimmed[first_split..].trim().to_string();
    if left.is_empty() || rest.is_empty() {
        return None;
    }

    let mut middle = String::new();
    if let Some(second_split) = rest.find("  ") {
        middle = rest[..second_split].trim().to_string();
        rest = rest[second_split..].trim().to_string();
    }
    Some((left, middle, rest))
}

fn split_flag_and_type(flag: &str) -> (String, String) {
    if let Some(start) = flag.rfind(" <")
        && flag.ends_with('>')
    {
        let left = flag[..start].trim().to_string();
        let ty = flag[start + 1..].trim().to_string();
        return (left, ty);
    }
    (flag.to_string(), String::new())
}

/// `ruff help` / `ruff help check` / `ruff help analyze graph`
fn resolve_help_subcommand_path(args: &[OsString]) -> Option<Vec<String>> {
    if args.get(1).map(OsString::as_os_str) != Some(OsStr::new("help")) {
        return None;
    }
    let mut path = Vec::new();
    for arg in args.iter().skip(2) {
        let Some(value) = arg.to_str() else {
            return None;
        };
        if value.starts_with('-') {
            break;
        }
        path.push(value.to_string());
    }
    Some(path)
}

/// `ruff -h`, `ruff check -h`, `ruff --help`, etc.
fn resolve_help_flag_path(args: &[OsString]) -> Option<Vec<String>> {
    if !args.iter().any(|arg| arg == "-h" || arg == "--help") {
        return None;
    }

    let mut path = Vec::new();
    for arg in args.iter().skip(1) {
        if arg == "-h" || arg == "--help" {
            break;
        }
        let Some(value) = arg.to_str() else {
            continue;
        };
        if value.starts_with('-') {
            break;
        }
        path.push(value.to_string());
    }
    Some(path)
}

fn resolve_block_help_path(args: &[OsString]) -> Option<Vec<String>> {
    if let Some(path) = resolve_help_subcommand_path(args) {
        return Some(path);
    }
    resolve_help_flag_path(args)
}

pub(crate) fn render_help_if_requested(args: &[OsString], writer: &mut dyn Write) -> Result<bool> {
    let Some(path) = resolve_block_help_path(args) else {
        return Ok(false);
    };

    let mut command = Args::command();
    for part in &path {
        let Some(next) = command.find_subcommand_mut(part) else {
            return Ok(false);
        };
        command = next.clone();
    }

    let usage_rendered = command.render_usage().to_string();
    let usage = usage_rendered
        .trim()
        .strip_prefix("Usage:")
        .map(str::trim)
        .unwrap_or(usage_rendered.trim())
        .to_string();
    let rendered = command.render_help().to_string();
    let use_color = colored::control::SHOULD_COLORIZE.should_colorize();

    let mut lines = rendered.lines();
    let mut about_lines = Vec::new();
    for line in lines.by_ref() {
        if line.trim().is_empty() {
            break;
        }
        about_lines.push(line.to_string());
    }

    for about in &about_lines {
        if use_color {
            writeln!(writer, "{}", about.green().bold())?;
        } else {
            writeln!(writer, "{about}")?;
        }
    }
    writeln!(writer)?;
    write_text_block(writer, "Usage", usage.trim(), use_color, true)?;
    writeln!(writer)?;

    let mut sections = Vec::new();
    let mut current = Section::default();
    for line in rendered.lines() {
        if line.starts_with("Usage:") || line.starts_with("For help with") {
            continue;
        }
        if let Some(title) = parse_section_title(line) {
            if !current.title.is_empty() {
                sections.push(current);
            }
            current = Section {
                title,
                lines: Vec::new(),
            };
            continue;
        }
        if !current.title.is_empty() {
            current.lines.push(line.to_string());
        }
    }
    if !current.title.is_empty() {
        sections.push(current);
    }

    for section in sections {
        let mut rows: Vec<(String, String, String)> = Vec::new();
        for raw in section.lines {
            if raw.trim().is_empty() {
                continue;
            }
            if let Some((first, middle, third)) = split_columns(&raw) {
                let (first, extra_type) = split_flag_and_type(&first);
                let typ = if middle.is_empty() {
                    extra_type
                } else {
                    middle
                };
                rows.push((first, typ, third));
            } else if raw.trim_start().starts_with('-') || raw.trim_start().starts_with('[') {
                let (first, typ) = split_flag_and_type(raw.trim());
                rows.push((first, typ, String::new()));
            } else if let Some((_, _, last)) = rows.last_mut() {
                if !last.is_empty() {
                    last.push(' ');
                }
                last.push_str(raw.trim());
            }
        }
        if !rows.is_empty() {
            write_three_col_block(writer, &section.title, &rows, use_color)?;
            writeln!(writer)?;
        }
    }

    if path.is_empty() {
        let footer = "For help with a specific command, see: `ruff help <command>`.";
        if use_color {
            writeln!(writer, "{}", footer.green().bold())?;
        } else {
            writeln!(writer, "{footer}")?;
        }
    }

    Ok(true)
}
