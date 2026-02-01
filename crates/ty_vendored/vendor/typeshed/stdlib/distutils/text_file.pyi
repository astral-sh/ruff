"""text_file

provides the TextFile class, which gives an interface to text files
that (optionally) takes care of stripping comments, ignoring blank
lines, and joining lines with backslashes.
"""

from typing import IO, Literal

class TextFile:
    """Provides a file-like object that takes care of all the things you
    commonly want to do when processing a text file that has some
    line-by-line syntax: strip comments (as long as "#" is your
    comment character), skip blank lines, join adjacent lines by
    escaping the newline (ie. backslash at end of line), strip
    leading and/or trailing whitespace.  All of these are optional
    and independently controllable.

    Provides a 'warn()' method so you can generate warning messages that
    report physical line number, even if the logical line in question
    spans multiple physical lines.  Also provides 'unreadline()' for
    implementing line-at-a-time lookahead.

    Constructor is called as:

        TextFile (filename=None, file=None, **options)

    It bombs (RuntimeError) if both 'filename' and 'file' are None;
    'filename' should be a string, and 'file' a file object (or
    something that provides 'readline()' and 'close()' methods).  It is
    recommended that you supply at least 'filename', so that TextFile
    can include it in warning messages.  If 'file' is not supplied,
    TextFile creates its own using 'io.open()'.

    The options are all boolean, and affect the value returned by
    'readline()':
      strip_comments [default: true]
        strip from "#" to end-of-line, as well as any whitespace
        leading up to the "#" -- unless it is escaped by a backslash
      lstrip_ws [default: false]
        strip leading whitespace from each line before returning it
      rstrip_ws [default: true]
        strip trailing whitespace (including line terminator!) from
        each line before returning it
      skip_blanks [default: true}
        skip lines that are empty *after* stripping comments and
        whitespace.  (If both lstrip_ws and rstrip_ws are false,
        then some lines may consist of solely whitespace: these will
        *not* be skipped, even if 'skip_blanks' is true.)
      join_lines [default: false]
        if a backslash is the last non-newline character on a line
        after stripping comments and whitespace, join the following line
        to it to form one "logical line"; if N consecutive lines end
        with a backslash, then N+1 physical lines will be joined to
        form one logical line.
      collapse_join [default: false]
        strip leading whitespace from lines that are joined to their
        predecessor; only matters if (join_lines and not lstrip_ws)
      errors [default: 'strict']
        error handler used to decode the file content

    Note that since 'rstrip_ws' can strip the trailing newline, the
    semantics of 'readline()' must differ from those of the builtin file
    object's 'readline()' method!  In particular, 'readline()' returns
    None for end-of-file: an empty string might just be a blank line (or
    an all-whitespace line), if 'rstrip_ws' is true but 'skip_blanks' is
    not.
    """

    def __init__(
        self,
        filename: str | None = None,
        file: IO[str] | None = None,
        *,
        strip_comments: bool | Literal[0, 1] = ...,
        lstrip_ws: bool | Literal[0, 1] = ...,
        rstrip_ws: bool | Literal[0, 1] = ...,
        skip_blanks: bool | Literal[0, 1] = ...,
        join_lines: bool | Literal[0, 1] = ...,
        collapse_join: bool | Literal[0, 1] = ...,
    ) -> None:
        """Construct a new TextFile object.  At least one of 'filename'
        (a string) and 'file' (a file-like object) must be supplied.
        They keyword argument options are described above and affect
        the values returned by 'readline()'.
        """

    def open(self, filename: str) -> None:
        """Open a new file named 'filename'.  This overrides both the
        'filename' and 'file' arguments to the constructor.
        """

    def close(self) -> None:
        """Close the current file and forget everything we know about it
        (filename, current line number).
        """

    def warn(self, msg: str, line: list[int] | tuple[int, int] | int | None = None) -> None:
        """Print (to stderr) a warning message tied to the current logical
        line in the current file.  If the current logical line in the
        file spans multiple physical lines, the warning refers to the
        whole range, eg. "lines 3-5".  If 'line' supplied, it overrides
        the current line number; it may be a list or tuple to indicate a
        range of physical lines, or an integer for a single physical
        line.
        """

    def readline(self) -> str | None:
        """Read and return a single logical line from the current file (or
        from an internal buffer if lines have previously been "unread"
        with 'unreadline()').  If the 'join_lines' option is true, this
        may involve reading multiple physical lines concatenated into a
        single string.  Updates the current line number, so calling
        'warn()' after 'readline()' emits a warning about the physical
        line(s) just read.  Returns None on end-of-file, since the empty
        string can occur if 'rstrip_ws' is true but 'strip_blanks' is
        not.
        """

    def readlines(self) -> list[str]:
        """Read and return the list of all logical lines remaining in the
        current file.
        """

    def unreadline(self, line: str) -> str:
        """Push 'line' (a string) onto an internal buffer that will be
        checked by future 'readline()' calls.  Handy for implementing
        a parser with line-at-a-time lookahead.
        """
