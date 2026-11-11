//! A simple ASCII table renderer for terminal output.
//!
//! Inspired by the [`ascii_table_rs`](https://docs.rs/crate/ascii_table_rs/latest/source/)
//! crate. Uses plain ASCII dashes for separators rather than box-drawing characters.
//!
//! # Example
//!
//! ```
//! use ty::coverage::table::{Align, AsciiTable, Column};
//! let mut table = AsciiTable::new(vec![
//!     Column::new("File", Align::Left),
//!     Column::new("Total", Align::Right),
//! ]);
//! table.push_row(vec!["foo.py".into(), "42".into()]);
//! table.set_footer(vec!["Total".into(), "42".into()]);
//! table.render(&mut std::io::stdout()).unwrap();
//! ```

use std::io;

/// Column alignment.
pub enum Align {
    Left,
    Right,
}

/// A column definition: header text and alignment.
pub struct Column {
    header: &'static str,
    align: Align,
}

impl Column {
    pub fn new(header: &'static str, align: Align) -> Self {
        Self { header, align }
    }
}

/// An ASCII table with a header, data rows, and an optional footer row.
///
/// Column widths are computed automatically from the widest value in each column.
/// Rows and the footer are separated from the header by dash separator lines.
pub struct AsciiTable {
    columns: Vec<Column>,
    rows: Vec<Vec<String>>,
    footer: Option<Vec<String>>,
}

impl AsciiTable {
    pub fn new(columns: Vec<Column>) -> Self {
        Self {
            columns,
            rows: Vec::new(),
            footer: None,
        }
    }

    pub fn push_row(&mut self, row: Vec<String>) {
        self.rows.push(row);
    }

    pub fn set_footer(&mut self, footer: Vec<String>) {
        self.footer = Some(footer);
    }

    /// Render the table to `out`.
    pub fn render(&self, out: &mut impl io::Write) -> io::Result<()> {
        let n = self.columns.len();

        // Compute column widths: max of header, all data rows, and footer.
        let mut widths: Vec<usize> = self.columns.iter().map(|c| c.header.len()).collect();
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate().take(n) {
                widths[i] = widths[i].max(cell.len());
            }
        }
        if let Some(footer) = &self.footer {
            for (i, cell) in footer.iter().enumerate().take(n) {
                widths[i] = widths[i].max(cell.len());
            }
        }

        let format_row = |cells: &[&str]| -> String {
            let parts: Vec<String> = self
                .columns
                .iter()
                .zip(&widths)
                .enumerate()
                .map(|(i, (col, &w))| {
                    let val = cells.get(i).copied().unwrap_or("");
                    match col.align {
                        Align::Left => format!("{val:<w$}"),
                        Align::Right => format!("{val:>w$}"),
                    }
                })
                .collect();
            parts.join("  ")
        };

        let sep: String = widths
            .iter()
            .map(|&w| "-".repeat(w))
            .collect::<Vec<_>>()
            .join("  ");

        // Header
        let header_strs: Vec<&str> = self.columns.iter().map(|c| c.header).collect();
        writeln!(out, "{}", format_row(&header_strs))?;

        writeln!(out, "{sep}")?;

        for row in &self.rows {
            let cells: Vec<&str> = row.iter().map(String::as_str).collect();
            writeln!(out, "{}", format_row(&cells))?;
        }

        if let Some(footer) = &self.footer {
            writeln!(out, "{sep}")?;
            let cells: Vec<&str> = footer.iter().map(String::as_str).collect();
            writeln!(out, "{}", format_row(&cells))?;
        }

        Ok(())
    }
}
