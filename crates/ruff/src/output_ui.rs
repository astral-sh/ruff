use std::borrow::Cow;
use std::io::{self, Write};

use colored::Colorize;

#[derive(Copy, Clone)]
struct Borders {
    top_left: char,
    top_right: char,
    bottom_left: char,
    bottom_right: char,
    horizontal: char,
    vertical: char,
}

const UNICODE_BORDERS: Borders = Borders {
    top_left: '╭',
    top_right: '╮',
    bottom_left: '╰',
    bottom_right: '╯',
    horizontal: '─',
    vertical: '│',
};

const ASCII_BORDERS: Borders = Borders {
    top_left: '+',
    top_right: '+',
    bottom_left: '+',
    bottom_right: '+',
    horizontal: '-',
    vertical: '|',
};

fn terminal_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|columns| columns.parse::<usize>().ok())
        .map(|columns| columns.clamp(72, 140))
        .unwrap_or(100)
}

/// Visible width of a string, ignoring common ANSI SGR sequences (help text is ASCII-heavy).
fn visible_width(s: &str) -> usize {
    let mut width = 0;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(ch) = chars.next() {
                    if ch == 'm' {
                        break;
                    }
                }
            }
            continue;
        }
        width += unicode_column_width(c);
    }
    width
}

fn unicode_column_width(c: char) -> usize {
    usize::from(!c.is_control())
}

fn pad_to_visible_width(s: &str, target: usize) -> String {
    let pad = target.saturating_sub(visible_width(s));
    format!("{s}{}", " ".repeat(pad))
}

fn wrap_text(value: &str, width: usize) -> Vec<Cow<'_, str>> {
    if width == 0 || value.len() <= width {
        return vec![Cow::Borrowed(value)];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in value.split_whitespace() {
        let separator = usize::from(!current.is_empty());
        if current.len() + separator + word.len() > width && !current.is_empty() {
            lines.push(Cow::Owned(current));
            current = String::new();
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(Cow::Owned(current));
    }
    if lines.is_empty() {
        vec![Cow::Borrowed("")]
    } else {
        lines
    }
}

fn write_border(
    writer: &mut dyn Write,
    left: char,
    fill: char,
    right: char,
    width: usize,
    colored: bool,
) -> io::Result<()> {
    let border = format!(
        "{left}{}{right}",
        fill.to_string().repeat(width.saturating_sub(2))
    );
    if colored {
        writeln!(writer, "{}", border.cyan())
    } else {
        writeln!(writer, "{border}")
    }
}

fn write_inner_line(
    writer: &mut dyn Write,
    left: char,
    right: char,
    content: &str,
    inner_width: usize,
    colored: bool,
) -> io::Result<()> {
    let rendered = pad_to_visible_width(content, inner_width);
    if colored {
        writeln!(
            writer,
            "{}{}{}",
            left.to_string().cyan(),
            rendered,
            right.to_string().cyan()
        )
    } else {
        writeln!(writer, "{left}{rendered}{right}")
    }
}

pub(crate) fn write_text_block(
    writer: &mut dyn Write,
    title: &str,
    body: &str,
    colored: bool,
    color_body: bool,
) -> io::Result<()> {
    let width = terminal_width();
    let borders = if colored {
        UNICODE_BORDERS
    } else {
        ASCII_BORDERS
    };
    let inner_width = width.saturating_sub(2);

    write_border(
        writer,
        borders.top_left,
        borders.horizontal,
        borders.top_right,
        width,
        colored,
    )?;
    let title_line = if colored {
        format!(" {}", title.green().bold())
    } else {
        format!(" {title}")
    };
    write_inner_line(
        writer,
        borders.vertical,
        borders.vertical,
        &title_line,
        inner_width,
        colored,
    )?;
    write_inner_line(
        writer,
        borders.vertical,
        borders.vertical,
        "",
        inner_width,
        colored,
    )?;
    for line in body.lines() {
        for wrapped in wrap_text(line, inner_width.saturating_sub(1)) {
            let body_line = if colored && color_body {
                format!(" {}", wrapped.green().bold())
            } else {
                format!(" {wrapped}")
            };
            write_inner_line(
                writer,
                borders.vertical,
                borders.vertical,
                &body_line,
                inner_width,
                colored,
            )?;
        }
    }
    write_border(
        writer,
        borders.bottom_left,
        borders.horizontal,
        borders.bottom_right,
        width,
        colored,
    )
}

pub(crate) fn write_two_col_block(
    writer: &mut dyn Write,
    title: &str,
    rows: &[(String, String)],
    colored: bool,
) -> io::Result<()> {
    let width = terminal_width();
    let borders = if colored {
        UNICODE_BORDERS
    } else {
        ASCII_BORDERS
    };
    let inner_width = width.saturating_sub(2);
    let key_width = rows
        .iter()
        .map(|(key, _)| key.len())
        .max()
        .unwrap_or(0)
        .min(28);
    let value_width = inner_width.saturating_sub(key_width + 4);

    write_border(
        writer,
        borders.top_left,
        borders.horizontal,
        borders.top_right,
        width,
        colored,
    )?;
    let title_line = if colored {
        format!(" {}", title.green().bold())
    } else {
        format!(" {title}")
    };
    write_inner_line(
        writer,
        borders.vertical,
        borders.vertical,
        &title_line,
        inner_width,
        colored,
    )?;
    write_inner_line(
        writer,
        borders.vertical,
        borders.vertical,
        "",
        inner_width,
        colored,
    )?;
    for (key, value) in rows {
        let wrapped = wrap_text(value, value_width);
        for (index, line) in wrapped.into_iter().enumerate() {
            let key_cell = if index == 0 {
                if colored {
                    pad_to_visible_width(&format!("{}", key.cyan().bold()), key_width)
                } else {
                    format!("{key:<key_width$}")
                }
            } else {
                " ".repeat(key_width)
            };
            write_inner_line(
                writer,
                borders.vertical,
                borders.vertical,
                &format!(" {}  {line}", key_cell),
                inner_width,
                colored,
            )?;
        }
    }
    write_border(
        writer,
        borders.bottom_left,
        borders.horizontal,
        borders.bottom_right,
        width,
        colored,
    )
}

pub(crate) fn write_three_col_block(
    writer: &mut dyn Write,
    title: &str,
    rows: &[(String, String, String)],
    colored: bool,
) -> io::Result<()> {
    let width = terminal_width();
    let borders = if colored {
        UNICODE_BORDERS
    } else {
        ASCII_BORDERS
    };
    let inner_width = width.saturating_sub(2);
    let first_width = rows
        .iter()
        .map(|(first, _, _)| first.len())
        .max()
        .unwrap_or(0)
        .min(28);
    let second_width = rows
        .iter()
        .map(|(_, second, _)| second.len())
        .max()
        .unwrap_or(0)
        .min(14);
    let third_width = inner_width.saturating_sub(first_width + second_width + 6);

    write_border(
        writer,
        borders.top_left,
        borders.horizontal,
        borders.top_right,
        width,
        colored,
    )?;
    let title_line = if colored {
        format!(" {}", title.green().bold())
    } else {
        format!(" {title}")
    };
    write_inner_line(
        writer,
        borders.vertical,
        borders.vertical,
        &title_line,
        inner_width,
        colored,
    )?;
    write_inner_line(
        writer,
        borders.vertical,
        borders.vertical,
        "",
        inner_width,
        colored,
    )?;

    for (first, second, third) in rows {
        let wrapped = wrap_text(third, third_width);
        for (index, line) in wrapped.into_iter().enumerate() {
            let first_value = if index == 0 { first.as_str() } else { "" };
            let second_value = if index == 0 { second.as_str() } else { "" };
            let first_cell = if first_value.is_empty() {
                " ".repeat(first_width)
            } else if colored {
                pad_to_visible_width(&format!("{}", first_value.cyan().bold()), first_width)
            } else {
                format!("{first_value:<first_width$}")
            };
            let second_cell = if second_value.is_empty() {
                " ".repeat(second_width)
            } else if colored {
                pad_to_visible_width(&format!("{}", second_value.yellow()), second_width)
            } else {
                format!("{second_value:<second_width$}")
            };
            write_inner_line(
                writer,
                borders.vertical,
                borders.vertical,
                &format!(" {}  {}  {}", first_cell, second_cell, line),
                inner_width,
                colored,
            )?;
        }
    }
    write_border(
        writer,
        borders.bottom_left,
        borders.horizontal,
        borders.bottom_right,
        width,
        colored,
    )
}
