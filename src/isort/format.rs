use ropey::RopeBuilder;

use crate::isort::types::{AliasData, CommentSet, ImportFromData, Importable};

// Hard-code four-space indentation for the imports themselves, to match Black.
const INDENT: &str = "    ";

/// Add a plain import statement to the `RopeBuilder`.
pub fn format_import(output: &mut RopeBuilder, alias: &AliasData, comments: &CommentSet) {
    for comment in &comments.atop {
        output.append(&format!("{}\n", comment));
    }
    if let Some(asname) = alias.asname {
        output.append(&format!("import {} as {}", alias.name, asname));
    } else {
        output.append(&format!("import {}", alias.name));
    }
    for comment in &comments.inline {
        output.append(&format!("  {}", comment));
    }
    output.append("\n");
}

/// Add an import-from statement to the `RopeBuilder`.
pub fn format_import_from(
    output: &mut RopeBuilder,
    import_from: &ImportFromData,
    comments: &CommentSet,
    aliases: &[(AliasData, CommentSet)],
    line_length: &usize,
) {
    // We can only inline if: (1) none of the aliases have atop comments, and (3)
    // only the last alias (if any) has inline comments.
    if aliases
        .iter()
        .all(|(_, CommentSet { atop, .. })| atop.is_empty())
        && aliases
            .iter()
            .rev()
            .skip(1)
            .all(|(_, CommentSet { inline, .. })| inline.is_empty())
    {
        // STOPSHIP(charlie): This includes the length of the comments...
        let (single_line, import_length) = format_single_line(import_from, comments, aliases);
        // If the import fits on a single line (excluding the newline character at the
        // end, which doesn't count towards the line length), return it.
        if import_length <= *line_length {
            output.append(&single_line);
            return;
        }
    }

    output.append(&format_multi_line(import_from, comments, aliases));
}

/// Format an import-from statement in single-line format.
///
/// This method assumes that the output source code is syntactically valid.
fn format_single_line(
    import_from: &ImportFromData,
    comments: &CommentSet,
    aliases: &[(AliasData, CommentSet)],
) -> (String, usize) {
    let mut output = String::new();
    let mut import_length = 0;

    for comment in &comments.atop {
        output.push_str(comment);
        output.push('\n');
    }

    output.push_str(&format!("from {} import ", import_from.module_name()));

    for (index, (AliasData { name, asname }, comments)) in aliases.iter().enumerate() {
        for comment in &comments.atop {
            output.push_str(comment);
            output.push('\n');
        }
        if let Some(asname) = asname {
            output.push_str(name);
            output.push_str(" as ");
            output.push_str(asname);
        } else {
            output.push_str(name);
        }
        if index < aliases.len() - 1 {
            output.push_str(", ");
        }

        for comment in &comments.inline {
            output.push_str(&format!("  {}", comment));
        }
    }

    for comment in &comments.inline {
        output.push_str(&format!("  {}", comment));
    }

    output.push('\n');

    output
}

/// Format an import-from statement in multi-line format.
fn format_multi_line(
    import_from: &ImportFromData,
    comments: &CommentSet,
    aliases: &[(AliasData, CommentSet)],
) -> String {
    let mut output = String::new();

    for comment in &comments.atop {
        output.push_str(comment);
        output.push('\n');
    }

    output.push_str(&format!("from {} import ", import_from.module_name()));
    output.push('(');
    for comment in &comments.inline {
        output.push_str(&format!("  {}", comment));
    }
    output.push('\n');

    for (AliasData { name, asname }, comments) in aliases {
        for comment in &comments.atop {
            output.push_str(INDENT);
            output.push_str(comment);
            output.push('\n');
        }
        output.push_str(INDENT);
        if let Some(asname) = asname {
            output.push_str(name);
            output.push_str(" as ");
            output.push_str(asname);
        } else {
            output.push_str(name);
        }
        output.push(',');

        for comment in &comments.inline {
            output.push(' ');
            output.push(' ');
            output.push_str(comment);
        }
        output.push('\n');
    }

    output.push(')');
    output.push('\n');

    output
}
