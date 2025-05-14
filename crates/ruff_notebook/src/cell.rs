use std::fmt;
use std::ops::{Deref, DerefMut};

use itertools::Itertools;

use ruff_text_size::{TextRange, TextSize};

use crate::schema::{Cell, SourceValue};
use crate::CellMetadata;

impl fmt::Display for SourceValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceValue::String(string) => f.write_str(string),
            SourceValue::StringArray(string_array) => {
                for string in string_array {
                    f.write_str(string)?;
                }
                Ok(())
            }
        }
    }
}

impl Cell {
    /// Return the [`SourceValue`] of the cell.
    pub fn source(&self) -> &SourceValue {
        match self {
            Cell::Code(cell) => &cell.source,
            Cell::Markdown(cell) => &cell.source,
            Cell::Raw(cell) => &cell.source,
        }
    }

    pub fn is_code_cell(&self) -> bool {
        matches!(self, Cell::Code(_))
    }

    pub fn metadata(&self) -> &CellMetadata {
        match self {
            Cell::Code(cell) => &cell.metadata,
            Cell::Markdown(cell) => &cell.metadata,
            Cell::Raw(cell) => &cell.metadata,
        }
    }

    /// Update the [`SourceValue`] of the cell.
    pub(crate) fn set_source(&mut self, source: SourceValue) {
        match self {
            Cell::Code(cell) => cell.source = source,
            Cell::Markdown(cell) => cell.source = source,
            Cell::Raw(cell) => cell.source = source,
        }
    }

    /// Return `true` if it's a valid code cell.
    ///
    /// A valid code cell is a cell where:
    /// 1. The cell type is [`Cell::Code`]
    /// 2. The source doesn't contain a cell magic
    /// 3. If the language id is set, it should be `python`
    pub(crate) fn is_valid_python_code_cell(&self) -> bool {
        let source = match self {
            Cell::Code(cell)
                if cell
                    .metadata
                    .vscode
                    .as_ref()
                    .is_none_or(|vscode| vscode.language_id == "python") =>
            {
                &cell.source
            }
            _ => return false,
        };
        // Ignore cells containing cell magic as they act on the entire cell
        // as compared to line magic which acts on a single line.
        !match source {
            SourceValue::String(string) => Self::is_magic_cell(string.lines()),
            SourceValue::StringArray(string_array) => {
                Self::is_magic_cell(string_array.iter().map(String::as_str))
            }
        }
    }

    /// Returns `true` if a cell should be ignored due to the use of cell magics.
    fn is_magic_cell<'a>(lines: impl Iterator<Item = &'a str>) -> bool {
        let mut lines = lines.peekable();

        // Detect automatic line magics (automagic), which aren't supported by the parser. If a line
        // magic uses automagic, Jupyter doesn't allow following it with non-magic lines anyway, so
        // we aren't missing out on any valid Python code.
        //
        // For example, this is valid:
        // ```jupyter
        // cat /path/to/file
        // cat /path/to/file
        // ```
        //
        // But this is invalid:
        // ```jupyter
        // cat /path/to/file
        // x = 1
        // ```
        //
        // See: https://ipython.readthedocs.io/en/stable/interactive/magics.html
        if let Some(line) = lines.peek() {
            let mut tokens = line.split_whitespace();

            // The first token must be an automagic, like `load_exit`.
            if tokens.next().is_some_and(|token| {
                matches!(
                    token,
                    "alias"
                        | "alias_magic"
                        | "autoawait"
                        | "autocall"
                        | "automagic"
                        | "bookmark"
                        | "cd"
                        | "code_wrap"
                        | "colors"
                        | "conda"
                        | "config"
                        | "debug"
                        | "dhist"
                        | "dirs"
                        | "doctest_mode"
                        | "edit"
                        | "env"
                        | "gui"
                        | "history"
                        | "killbgscripts"
                        | "load"
                        | "load_ext"
                        | "loadpy"
                        | "logoff"
                        | "logon"
                        | "logstart"
                        | "logstate"
                        | "logstop"
                        | "lsmagic"
                        | "macro"
                        | "magic"
                        | "mamba"
                        | "matplotlib"
                        | "micromamba"
                        | "notebook"
                        | "page"
                        | "pastebin"
                        | "pdb"
                        | "pdef"
                        | "pdoc"
                        | "pfile"
                        | "pinfo"
                        | "pinfo2"
                        | "pip"
                        | "popd"
                        | "pprint"
                        | "precision"
                        | "prun"
                        | "psearch"
                        | "psource"
                        | "pushd"
                        | "pwd"
                        | "pycat"
                        | "pylab"
                        | "quickref"
                        | "recall"
                        | "rehashx"
                        | "reload_ext"
                        | "rerun"
                        | "reset"
                        | "reset_selective"
                        | "run"
                        | "save"
                        | "sc"
                        | "set_env"
                        | "sx"
                        | "system"
                        | "tb"
                        | "time"
                        | "timeit"
                        | "unalias"
                        | "unload_ext"
                        | "who"
                        | "who_ls"
                        | "whos"
                        | "xdel"
                        | "xmode"
                )
            }) {
                // The second token must _not_ be an operator, like `=` (to avoid false positives).
                // The assignment operators can never follow an automagic. Some binary operators
                // _can_, though (e.g., `cd -` is valid), so we omit them.
                if !tokens.next().is_some_and(|token| {
                    matches!(
                        token,
                        "=" | "+=" | "-=" | "*=" | "/=" | "//=" | "%=" | "**=" | "&=" | "|=" | "^="
                    )
                }) {
                    return true;
                }
            }
        }

        // Detect cell magics (which operate on multiple lines).
        lines.any(|line| {
            let Some(first) = line.split_whitespace().next() else {
                return false;
            };
            if first.len() < 2 {
                return false;
            }
            let Some(command) = first.strip_prefix("%%") else {
                return false;
            };
            // These cell magics are special in that the lines following them are valid
            // Python code and the variables defined in that scope are available to the
            // rest of the notebook.
            //
            // For example:
            //
            // Cell 1:
            // ```python
            // x = 1
            // ```
            //
            // Cell 2:
            // ```python
            // %%time
            // y = x
            // ```
            //
            // Cell 3:
            // ```python
            // print(y)  # Here, `y` is available.
            // ```
            //
            // This is to avoid false positives when these variables are referenced
            // elsewhere in the notebook.
            //
            // Refer https://github.com/astral-sh/ruff/issues/13718 for `ipytest`.
            !matches!(
                command,
                "capture"
                    | "debug"
                    | "ipytest"
                    | "prun"
                    | "pypy"
                    | "python"
                    | "python3"
                    | "time"
                    | "timeit"
            )
        })
    }
}

/// Cell offsets are used to keep track of the start and end offsets of each
/// cell in the concatenated source code. These offsets are in sorted order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellOffsets(Vec<TextSize>);

impl CellOffsets {
    /// Create a new [`CellOffsets`] with the given capacity.
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    /// Push a new offset to the end of the [`CellOffsets`].
    ///
    /// # Panics
    ///
    /// Panics if the offset is less than the last offset pushed.
    pub(crate) fn push(&mut self, offset: TextSize) {
        if let Some(last_offset) = self.0.last() {
            assert!(
                *last_offset <= offset,
                "Offsets must be pushed in sorted order"
            );
        }
        self.0.push(offset);
    }

    /// Returns the range of the cell containing the given offset, if any.
    pub fn containing_range(&self, offset: TextSize) -> Option<TextRange> {
        self.iter().tuple_windows().find_map(|(start, end)| {
            if *start <= offset && offset < *end {
                Some(TextRange::new(*start, *end))
            } else {
                None
            }
        })
    }

    /// Returns `true` if the given range contains a cell boundary.
    pub fn has_cell_boundary(&self, range: TextRange) -> bool {
        self.binary_search_by(|offset| {
            if range.start() <= *offset {
                if range.end() < *offset {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            } else {
                std::cmp::Ordering::Less
            }
        })
        .is_ok()
    }
}

impl Deref for CellOffsets {
    type Target = [TextSize];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for CellOffsets {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
