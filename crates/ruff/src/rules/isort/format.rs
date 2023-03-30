use ruff_python_ast::source_code::Stylist;
use unicode_width::UnicodeWidthStr;

use super::types::{AliasData, CommentSet, ImportFromData, Importable};

// Guess a capacity to use for string allocation.
const CAPACITY: usize = 200;

/// Add a plain import statement to the [`RopeBuilder`].
pub fn format_import(
    alias: &AliasData,
    comments: &CommentSet,
    is_first: bool,
    stylist: &Stylist,
) -> String {
    let mut output = String::with_capacity(CAPACITY);
    if !is_first && !comments.atop.is_empty() {
        output.push_str(&stylist.line_ending());
    }
    for comment in &comments.atop {
        output.push_str(comment);
        output.push_str(&stylist.line_ending());
    }
    if let Some(asname) = alias.asname {
        output.push_str("import ");
        output.push_str(alias.name);
        output.push_str(" as ");
        output.push_str(asname);
    } else {
        output.push_str("import ");
        output.push_str(alias.name);
    }
    for comment in &comments.inline {
        output.push_str("  ");
        output.push_str(comment);
    }
    output.push_str(&stylist.line_ending());
    output
}

/// Add an import-from statement to the [`RopeBuilder`].
#[allow(clippy::too_many_arguments)]
pub fn format_import_from(
    import_from: &ImportFromData,
    comments: &CommentSet,
    aliases: &[(AliasData, CommentSet)],
    line_length: usize,
    stylist: &Stylist,
    force_wrap_aliases: bool,
    is_first: bool,
    trailing_comma: bool,
) -> String {
    if aliases.len() == 1
        && aliases
            .iter()
            .all(|(alias, _)| alias.name == "*" && alias.asname.is_none())
    {
        let (single_line, ..) =
            format_single_line(import_from, comments, aliases, is_first, stylist);
        return single_line;
    }

    // We can only inline if none of the aliases have atop or inline comments.
    if !trailing_comma
        && (aliases.len() == 1
            || aliases
                .iter()
                .all(|(_, CommentSet { atop, inline })| atop.is_empty() && inline.is_empty()))
        && (!force_wrap_aliases
            || aliases.len() == 1
            || aliases.iter().all(|(alias, _)| alias.asname.is_none()))
    {
        let (single_line, import_width) =
            format_single_line(import_from, comments, aliases, is_first, stylist);
        if import_width <= line_length || aliases.iter().any(|(alias, _)| alias.name == "*") {
            return single_line;
        }
    }

    format_multi_line(import_from, comments, aliases, is_first, stylist)
}

/// Format an import-from statement in single-line format.
///
/// This method assumes that the output source code is syntactically valid.
fn format_single_line(
    import_from: &ImportFromData,
    comments: &CommentSet,
    aliases: &[(AliasData, CommentSet)],
    is_first: bool,
    stylist: &Stylist,
) -> (String, usize) {
    let mut output = String::with_capacity(CAPACITY);
    let mut line_width = 0;

    if !is_first && !comments.atop.is_empty() {
        output.push_str(&stylist.line_ending());
    }
    for comment in &comments.atop {
        output.push_str(comment);
        output.push_str(&stylist.line_ending());
    }

    let module_name = import_from.module_name();
    output.push_str("from ");
    output.push_str(&module_name);
    output.push_str(" import ");
    line_width += 5 + module_name.width() + 8;

    for (index, (AliasData { name, asname }, comments)) in aliases.iter().enumerate() {
        if let Some(asname) = asname {
            output.push_str(name);
            output.push_str(" as ");
            output.push_str(asname);
            line_width += name.width() + 4 + asname.width();
        } else {
            output.push_str(name);
            line_width += name.width();
        }
        if index < aliases.len() - 1 {
            output.push_str(", ");
            line_width += 2;
        }

        for comment in &comments.inline {
            output.push(' ');
            output.push(' ');
            output.push_str(comment);
            line_width += 2 + comment.width();
        }
    }

    for comment in &comments.inline {
        output.push(' ');
        output.push(' ');
        output.push_str(comment);
        line_width += 2 + comment.width();
    }

    output.push_str(&stylist.line_ending());

    (output, line_width)
}

/// Format an import-from statement in multi-line format.
fn format_multi_line(
    import_from: &ImportFromData,
    comments: &CommentSet,
    aliases: &[(AliasData, CommentSet)],
    is_first: bool,
    stylist: &Stylist,
) -> String {
    let mut output = String::with_capacity(CAPACITY);

    if !is_first && !comments.atop.is_empty() {
        output.push_str(&stylist.line_ending());
    }
    for comment in &comments.atop {
        output.push_str(comment);
        output.push_str(&stylist.line_ending());
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
    output.push_str(&stylist.line_ending());

    for (AliasData { name, asname }, comments) in aliases {
        for comment in &comments.atop {
            output.push_str(stylist.indentation());
            output.push_str(comment);
            output.push_str(&stylist.line_ending());
        }
        output.push_str(stylist.indentation());
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
        output.push_str(&stylist.line_ending());
    }

    output.push(')');
    output.push_str(&stylist.line_ending());

    output
}
