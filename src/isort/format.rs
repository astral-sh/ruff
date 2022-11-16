use ropey::RopeBuilder;

use crate::isort::types::{AliasData, CommentSet, ImportFromData, Importable};

// Hard-code four-space indentation for the imports themselves, to match Black.
const INDENT: &str = "    ";

/// Add a plain import statement to the `RopeBuilder`.
pub fn format_import(
    output: &mut RopeBuilder,
    alias: &AliasData,
    comments: &CommentSet,
    is_first: bool,
) {
    if !is_first && !comments.atop.is_empty() {
        output.append("\n");
    }
    for comment in &comments.atop {
        output.append(comment);
        output.append("\n");
    }
    if let Some(asname) = alias.asname {
        output.append("import ");
        output.append(alias.name);
        output.append(" as ");
        output.append(asname);
    } else {
        output.append("import ");
        output.append(alias.name);
    }
    for comment in &comments.inline {
        output.append("  ");
        output.append(comment);
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
    is_first: bool,
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
        let (single_line, import_length) =
            format_single_line(import_from, comments, aliases, is_first);
        if import_length <= *line_length {
            output.append(&single_line);
            return;
        }
    }

    output.append(&format_multi_line(import_from, comments, aliases, is_first));
}

/// Format an import-from statement in single-line format.
///
/// This method assumes that the output source code is syntactically valid.
fn format_single_line(
    import_from: &ImportFromData,
    comments: &CommentSet,
    aliases: &[(AliasData, CommentSet)],
    is_first: bool,
) -> (String, usize) {
    let mut output = String::new();
    let mut line_length = 0;

    if !is_first && !comments.atop.is_empty() {
        output.push('\n');
    }
    for comment in &comments.atop {
        output.push_str(comment);
        output.push('\n');
    }

    let module_name = import_from.module_name();
    output.push_str("from ");
    output.push_str(&module_name);
    output.push_str(" import ");
    output.push_str(&content);
    line_length += 5 + content.len() + 8;

    for (index, (AliasData { name, asname }, comments)) in aliases.iter().enumerate() {
        if let Some(asname) = asname {
            output.push_str(name);
            output.push_str(" as ");
            output.push_str(asname);
            line_length += name.len() + 4 + asname.len();
        } else {
            output.push_str(name);
            line_length += name.len();
        }
        if index < aliases.len() - 1 {
            output.push_str(", ");
            line_length += 2;
        }

        for comment in &comments.inline {
            output.push(' ');
            output.push(' ');
            output.push_str(comment);
            line_length += 2 + comment.len();
        }
    }

    for comment in &comments.inline {
        output.push(' ');
        output.push(' ');
        output.push_str(comment);
        line_length += 2 + comment.len();
    }

    output.push('\n');

    (output, line_length)
}

/// Format an import-from statement in multi-line format.
fn format_multi_line(
    import_from: &ImportFromData,
    comments: &CommentSet,
    aliases: &[(AliasData, CommentSet)],
    is_first: bool,
) -> String {
    let mut output = String::new();

    if !is_first && !comments.atop.is_empty() {
        output.push('\n');
    }
    for comment in &comments.atop {
        output.push_str(comment);
        output.push('\n');
    }

    output.push_str("from ");
    output.push_str(&import_from.module_name());
    output.push_str(" import ");
    output.push('(');
    for comment in &comments.inline {
        output.push(' ');
        output.push(' ');
        output.push_str(comment);
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
