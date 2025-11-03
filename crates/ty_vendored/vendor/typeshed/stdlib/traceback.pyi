"""Extract, format and print information about Python stack traces."""

import sys
from _typeshed import SupportsWrite, Unused
from collections.abc import Generator, Iterable, Iterator, Mapping
from types import FrameType, TracebackType
from typing import Any, ClassVar, Literal, overload
from typing_extensions import Self, TypeAlias, deprecated

__all__ = [
    "extract_stack",
    "extract_tb",
    "format_exception",
    "format_exception_only",
    "format_list",
    "format_stack",
    "format_tb",
    "print_exc",
    "format_exc",
    "print_exception",
    "print_last",
    "print_stack",
    "print_tb",
    "clear_frames",
    "FrameSummary",
    "StackSummary",
    "TracebackException",
    "walk_stack",
    "walk_tb",
]

if sys.version_info >= (3, 14):
    __all__ += ["print_list"]

_FrameSummaryTuple: TypeAlias = tuple[str, int, str, str | None]

def print_tb(tb: TracebackType | None, limit: int | None = None, file: SupportsWrite[str] | None = None) -> None:
    """Print up to 'limit' stack trace entries from the traceback 'tb'.

    If 'limit' is omitted or None, all entries are printed.  If 'file'
    is omitted or None, the output goes to sys.stderr; otherwise
    'file' should be an open file or file-like object with a write()
    method.
    """

if sys.version_info >= (3, 10):
    @overload
    def print_exception(
        exc: type[BaseException] | None,
        /,
        value: BaseException | None = ...,
        tb: TracebackType | None = ...,
        limit: int | None = None,
        file: SupportsWrite[str] | None = None,
        chain: bool = True,
    ) -> None:
        """Print exception up to 'limit' stack trace entries from 'tb' to 'file'.

        This differs from print_tb() in the following ways: (1) if
        traceback is not None, it prints a header "Traceback (most recent
        call last):"; (2) it prints the exception type and value after the
        stack trace; (3) if type is SyntaxError and value has the
        appropriate format, it prints the line where the syntax error
        occurred with a caret on the next line indicating the approximate
        position of the error.
        """

    @overload
    def print_exception(
        exc: BaseException, /, *, limit: int | None = None, file: SupportsWrite[str] | None = None, chain: bool = True
    ) -> None: ...
    @overload
    def format_exception(
        exc: type[BaseException] | None,
        /,
        value: BaseException | None = ...,
        tb: TracebackType | None = ...,
        limit: int | None = None,
        chain: bool = True,
    ) -> list[str]:
        """Format a stack trace and the exception information.

        The arguments have the same meaning as the corresponding arguments
        to print_exception().  The return value is a list of strings, each
        ending in a newline and some containing internal newlines.  When
        these lines are concatenated and printed, exactly the same text is
        printed as does print_exception().
        """

    @overload
    def format_exception(exc: BaseException, /, *, limit: int | None = None, chain: bool = True) -> list[str]: ...

else:
    def print_exception(
        etype: type[BaseException] | None,
        value: BaseException | None,
        tb: TracebackType | None,
        limit: int | None = None,
        file: SupportsWrite[str] | None = None,
        chain: bool = True,
    ) -> None:
        """Print exception up to 'limit' stack trace entries from 'tb' to 'file'.

        This differs from print_tb() in the following ways: (1) if
        traceback is not None, it prints a header "Traceback (most recent
        call last):"; (2) it prints the exception type and value after the
        stack trace; (3) if type is SyntaxError and value has the
        appropriate format, it prints the line where the syntax error
        occurred with a caret on the next line indicating the approximate
        position of the error.
        """

    def format_exception(
        etype: type[BaseException] | None,
        value: BaseException | None,
        tb: TracebackType | None,
        limit: int | None = None,
        chain: bool = True,
    ) -> list[str]:
        """Format a stack trace and the exception information.

        The arguments have the same meaning as the corresponding arguments
        to print_exception().  The return value is a list of strings, each
        ending in a newline and some containing internal newlines.  When
        these lines are concatenated and printed, exactly the same text is
        printed as does print_exception().
        """

def print_exc(limit: int | None = None, file: SupportsWrite[str] | None = None, chain: bool = True) -> None:
    """Shorthand for 'print_exception(sys.exception(), limit=limit, file=file, chain=chain)'."""

def print_last(limit: int | None = None, file: SupportsWrite[str] | None = None, chain: bool = True) -> None:
    """This is a shorthand for 'print_exception(sys.last_exc, limit=limit, file=file, chain=chain)'."""

def print_stack(f: FrameType | None = None, limit: int | None = None, file: SupportsWrite[str] | None = None) -> None:
    """Print a stack trace from its invocation point.

    The optional 'f' argument can be used to specify an alternate
    stack frame at which to start. The optional 'limit' and 'file'
    arguments have the same meaning as for print_exception().
    """

def extract_tb(tb: TracebackType | None, limit: int | None = None) -> StackSummary:
    """
    Return a StackSummary object representing a list of
    pre-processed entries from traceback.

    This is useful for alternate formatting of stack traces.  If
    'limit' is omitted or None, all entries are extracted.  A
    pre-processed stack trace entry is a FrameSummary object
    containing attributes filename, lineno, name, and line
    representing the information that is usually printed for a stack
    trace.  The line is a string with leading and trailing
    whitespace stripped; if the source is not available it is None.
    """

def extract_stack(f: FrameType | None = None, limit: int | None = None) -> StackSummary:
    """Extract the raw traceback from the current stack frame.

    The return value has the same format as for extract_tb().  The
    optional 'f' and 'limit' arguments have the same meaning as for
    print_stack().  Each item in the list is a quadruple (filename,
    line number, function name, text), and the entries are in order
    from oldest to newest stack frame.
    """

def format_list(extracted_list: Iterable[FrameSummary | _FrameSummaryTuple]) -> list[str]:
    """Format a list of tuples or FrameSummary objects for printing.

    Given a list of tuples or FrameSummary objects as returned by
    extract_tb() or extract_stack(), return a list of strings ready
    for printing.

    Each string in the resulting list corresponds to the item with the
    same index in the argument list.  Each string ends in a newline;
    the strings may contain internal newlines as well, for those items
    whose source text line is not None.
    """

def print_list(extracted_list: Iterable[FrameSummary | _FrameSummaryTuple], file: SupportsWrite[str] | None = None) -> None:
    """Print the list of tuples as returned by extract_tb() or
    extract_stack() as a formatted stack trace to the given file.
    """

if sys.version_info >= (3, 13):
    @overload
    def format_exception_only(exc: BaseException | None, /, *, show_group: bool = False) -> list[str]:
        """Format the exception part of a traceback.

        The return value is a list of strings, each ending in a newline.

        The list contains the exception's message, which is
        normally a single string; however, for :exc:`SyntaxError` exceptions, it
        contains several lines that (when printed) display detailed information
        about where the syntax error occurred. Following the message, the list
        contains the exception's ``__notes__``.

        When *show_group* is ``True``, and the exception is an instance of
        :exc:`BaseExceptionGroup`, the nested exceptions are included as
        well, recursively, with indentation relative to their nesting depth.
        """

    @overload
    def format_exception_only(exc: Unused, /, value: BaseException | None, *, show_group: bool = False) -> list[str]: ...

elif sys.version_info >= (3, 10):
    @overload
    def format_exception_only(exc: BaseException | None, /) -> list[str]:
        """Format the exception part of a traceback.

        The return value is a list of strings, each ending in a newline.

        The list contains the exception's message, which is
        normally a single string; however, for :exc:`SyntaxError` exceptions, it
        contains several lines that (when printed) display detailed information
        about where the syntax error occurred. Following the message, the list
        contains the exception's ``__notes__``.
        """

    @overload
    def format_exception_only(exc: Unused, /, value: BaseException | None) -> list[str]: ...

else:
    def format_exception_only(etype: type[BaseException] | None, value: BaseException | None) -> list[str]:
        """Format the exception part of a traceback.

        The arguments are the exception type and value such as given by
        sys.last_type and sys.last_value. The return value is a list of
        strings, each ending in a newline.

        Normally, the list contains a single string; however, for
        SyntaxError exceptions, it contains several lines that (when
        printed) display detailed information about where the syntax
        error occurred.

        The message indicating which exception occurred is always the last
        string in the list.

        """

def format_exc(limit: int | None = None, chain: bool = True) -> str:
    """Like print_exc() but return a string."""

def format_tb(tb: TracebackType | None, limit: int | None = None) -> list[str]:
    """A shorthand for 'format_list(extract_tb(tb, limit))'."""

def format_stack(f: FrameType | None = None, limit: int | None = None) -> list[str]:
    """Shorthand for 'format_list(extract_stack(f, limit))'."""

def clear_frames(tb: TracebackType | None) -> None:
    """Clear all references to local variables in the frames of a traceback."""

def walk_stack(f: FrameType | None) -> Iterator[tuple[FrameType, int]]:
    """Walk a stack yielding the frame and line number for each frame.

    This will follow f.f_back from the given frame. If no frame is given, the
    current stack is used. Usually used with StackSummary.extract.
    """

def walk_tb(tb: TracebackType | None) -> Iterator[tuple[FrameType, int]]:
    """Walk a traceback yielding the frame and line number for each frame.

    This will follow tb.tb_next (and thus is in the opposite order to
    walk_stack). Usually used with StackSummary.extract.
    """

if sys.version_info >= (3, 11):
    class _ExceptionPrintContext:
        def indent(self) -> str: ...
        def emit(self, text_gen: str | Iterable[str], margin_char: str | None = None) -> Generator[str, None, None]: ...

class TracebackException:
    """An exception ready for rendering.

    The traceback module captures enough attributes from the original exception
    to this intermediary form to ensure that no references are held, while
    still being able to fully print or format it.

    max_group_width and max_group_depth control the formatting of exception
    groups. The depth refers to the nesting level of the group, and the width
    refers to the size of a single exception group's exceptions array. The
    formatted output is truncated when either limit is exceeded.

    Use `from_exception` to create TracebackException instances from exception
    objects, or the constructor to create TracebackException instances from
    individual components.

    - :attr:`__cause__` A TracebackException of the original *__cause__*.
    - :attr:`__context__` A TracebackException of the original *__context__*.
    - :attr:`exceptions` For exception groups - a list of TracebackException
      instances for the nested *exceptions*.  ``None`` for other exceptions.
    - :attr:`__suppress_context__` The *__suppress_context__* value from the
      original exception.
    - :attr:`stack` A `StackSummary` representing the traceback.
    - :attr:`exc_type` (deprecated) The class of the original traceback.
    - :attr:`exc_type_str` String display of exc_type
    - :attr:`filename` For syntax errors - the filename where the error
      occurred.
    - :attr:`lineno` For syntax errors - the linenumber where the error
      occurred.
    - :attr:`end_lineno` For syntax errors - the end linenumber where the error
      occurred. Can be `None` if not present.
    - :attr:`text` For syntax errors - the text where the error
      occurred.
    - :attr:`offset` For syntax errors - the offset into the text where the
      error occurred.
    - :attr:`end_offset` For syntax errors - the end offset into the text where
      the error occurred. Can be `None` if not present.
    - :attr:`msg` For syntax errors - the compiler error message.
    """

    __cause__: TracebackException | None
    __context__: TracebackException | None
    if sys.version_info >= (3, 11):
        exceptions: list[TracebackException] | None
    __suppress_context__: bool
    if sys.version_info >= (3, 11):
        __notes__: list[str] | None
    stack: StackSummary

    # These fields only exist for `SyntaxError`s, but there is no way to express that in the type system.
    filename: str
    lineno: str | None
    if sys.version_info >= (3, 10):
        end_lineno: str | None
    text: str
    offset: int
    if sys.version_info >= (3, 10):
        end_offset: int | None
    msg: str

    if sys.version_info >= (3, 13):
        @property
        def exc_type_str(self) -> str: ...
        @property
        @deprecated("Deprecated since Python 3.13. Use `exc_type_str` instead.")
        def exc_type(self) -> type[BaseException] | None: ...
    else:
        exc_type: type[BaseException]
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            exc_type: type[BaseException],
            exc_value: BaseException,
            exc_traceback: TracebackType | None,
            *,
            limit: int | None = None,
            lookup_lines: bool = True,
            capture_locals: bool = False,
            compact: bool = False,
            max_group_width: int = 15,
            max_group_depth: int = 10,
            save_exc_type: bool = True,
            _seen: set[int] | None = None,
        ) -> None: ...
    elif sys.version_info >= (3, 11):
        def __init__(
            self,
            exc_type: type[BaseException],
            exc_value: BaseException,
            exc_traceback: TracebackType | None,
            *,
            limit: int | None = None,
            lookup_lines: bool = True,
            capture_locals: bool = False,
            compact: bool = False,
            max_group_width: int = 15,
            max_group_depth: int = 10,
            _seen: set[int] | None = None,
        ) -> None: ...
    elif sys.version_info >= (3, 10):
        def __init__(
            self,
            exc_type: type[BaseException],
            exc_value: BaseException,
            exc_traceback: TracebackType | None,
            *,
            limit: int | None = None,
            lookup_lines: bool = True,
            capture_locals: bool = False,
            compact: bool = False,
            _seen: set[int] | None = None,
        ) -> None: ...
    else:
        def __init__(
            self,
            exc_type: type[BaseException],
            exc_value: BaseException,
            exc_traceback: TracebackType | None,
            *,
            limit: int | None = None,
            lookup_lines: bool = True,
            capture_locals: bool = False,
            _seen: set[int] | None = None,
        ) -> None: ...

    if sys.version_info >= (3, 11):
        @classmethod
        def from_exception(
            cls,
            exc: BaseException,
            *,
            limit: int | None = None,
            lookup_lines: bool = True,
            capture_locals: bool = False,
            compact: bool = False,
            max_group_width: int = 15,
            max_group_depth: int = 10,
        ) -> Self:
            """Create a TracebackException from an exception."""
    elif sys.version_info >= (3, 10):
        @classmethod
        def from_exception(
            cls,
            exc: BaseException,
            *,
            limit: int | None = None,
            lookup_lines: bool = True,
            capture_locals: bool = False,
            compact: bool = False,
        ) -> Self:
            """Create a TracebackException from an exception."""
    else:
        @classmethod
        def from_exception(
            cls, exc: BaseException, *, limit: int | None = None, lookup_lines: bool = True, capture_locals: bool = False
        ) -> Self:
            """Create a TracebackException from an exception."""

    def __eq__(self, other: object) -> bool: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]
    if sys.version_info >= (3, 11):
        def format(self, *, chain: bool = True, _ctx: _ExceptionPrintContext | None = None) -> Generator[str, None, None]:
            """Format the exception.

            If chain is not *True*, *__cause__* and *__context__* will not be formatted.

            The return value is a generator of strings, each ending in a newline and
            some containing internal newlines. `print_exception` is a wrapper around
            this method which just prints the lines to a file.

            The message indicating which exception occurred is always the last
            string in the output.
            """
    else:
        def format(self, *, chain: bool = True) -> Generator[str, None, None]:
            """Format the exception.

            If chain is not *True*, *__cause__* and *__context__* will not be formatted.

            The return value is a generator of strings, each ending in a newline and
            some containing internal newlines. `print_exception` is a wrapper around
            this method which just prints the lines to a file.

            The message indicating which exception occurred is always the last
            string in the output.
            """
    if sys.version_info >= (3, 13):
        def format_exception_only(self, *, show_group: bool = False, _depth: int = 0) -> Generator[str, None, None]:
            """Format the exception part of the traceback.

            The return value is a generator of strings, each ending in a newline.

            Generator yields the exception message.
            For :exc:`SyntaxError` exceptions, it
            also yields (before the exception message)
            several lines that (when printed)
            display detailed information about where the syntax error occurred.
            Following the message, generator also yields
            all the exception's ``__notes__``.

            When *show_group* is ``True``, and the exception is an instance of
            :exc:`BaseExceptionGroup`, the nested exceptions are included as
            well, recursively, with indentation relative to their nesting depth.
            """
    else:
        def format_exception_only(self) -> Generator[str, None, None]:
            """Format the exception part of the traceback.

            The return value is a generator of strings, each ending in a newline.

            Generator yields the exception message.
            For :exc:`SyntaxError` exceptions, it
            also yields (before the exception message)
            several lines that (when printed)
            display detailed information about where the syntax error occurred.
            Following the message, generator also yields
            all the exception's ``__notes__``.
            """
    if sys.version_info >= (3, 11):
        def print(self, *, file: SupportsWrite[str] | None = None, chain: bool = True) -> None:
            """Print the result of self.format(chain=chain) to 'file'."""

class FrameSummary:
    """Information about a single frame from a traceback.

    - :attr:`filename` The filename for the frame.
    - :attr:`lineno` The line within filename for the frame that was
      active when the frame was captured.
    - :attr:`name` The name of the function or method that was executing
      when the frame was captured.
    - :attr:`line` The text from the linecache module for the
      of code that was running when the frame was captured.
    - :attr:`locals` Either None if locals were not supplied, or a dict
      mapping the name to the repr() of the variable.
    """

    if sys.version_info >= (3, 13):
        __slots__ = (
            "filename",
            "lineno",
            "end_lineno",
            "colno",
            "end_colno",
            "name",
            "_lines",
            "_lines_dedented",
            "locals",
            "_code",
        )
    elif sys.version_info >= (3, 11):
        __slots__ = ("filename", "lineno", "end_lineno", "colno", "end_colno", "name", "_line", "locals")
    else:
        __slots__ = ("filename", "lineno", "name", "_line", "locals")
    if sys.version_info >= (3, 11):
        def __init__(
            self,
            filename: str,
            lineno: int | None,
            name: str,
            *,
            lookup_line: bool = True,
            locals: Mapping[str, str] | None = None,
            line: str | None = None,
            end_lineno: int | None = None,
            colno: int | None = None,
            end_colno: int | None = None,
        ) -> None:
            """Construct a FrameSummary.

            :param lookup_line: If True, `linecache` is consulted for the source
                code line. Otherwise, the line will be looked up when first needed.
            :param locals: If supplied the frame locals, which will be captured as
                object representations.
            :param line: If provided, use this instead of looking up the line in
                the linecache.
            """
        end_lineno: int | None
        colno: int | None
        end_colno: int | None
    else:
        def __init__(
            self,
            filename: str,
            lineno: int | None,
            name: str,
            *,
            lookup_line: bool = True,
            locals: Mapping[str, str] | None = None,
            line: str | None = None,
        ) -> None:
            """Construct a FrameSummary.

            :param lookup_line: If True, `linecache` is consulted for the source
                code line. Otherwise, the line will be looked up when first needed.
            :param locals: If supplied the frame locals, which will be captured as
                object representations.
            :param line: If provided, use this instead of looking up the line in
                the linecache.
            """
    filename: str
    lineno: int | None
    name: str
    locals: dict[str, str] | None
    @property
    def line(self) -> str | None: ...
    @overload
    def __getitem__(self, pos: Literal[0]) -> str: ...
    @overload
    def __getitem__(self, pos: Literal[1]) -> int: ...
    @overload
    def __getitem__(self, pos: Literal[2]) -> str: ...
    @overload
    def __getitem__(self, pos: Literal[3]) -> str | None: ...
    @overload
    def __getitem__(self, pos: int) -> Any: ...
    @overload
    def __getitem__(self, pos: slice) -> tuple[Any, ...]: ...
    def __iter__(self) -> Iterator[Any]: ...
    def __eq__(self, other: object) -> bool: ...
    def __len__(self) -> Literal[4]: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]

class StackSummary(list[FrameSummary]):
    """A list of FrameSummary objects, representing a stack of frames."""

    @classmethod
    def extract(
        cls,
        frame_gen: Iterable[tuple[FrameType, int]],
        *,
        limit: int | None = None,
        lookup_lines: bool = True,
        capture_locals: bool = False,
    ) -> StackSummary:
        """Create a StackSummary from a traceback or stack object.

        :param frame_gen: A generator that yields (frame, lineno) tuples
            whose summaries are to be included in the stack.
        :param limit: None to include all frames or the number of frames to
            include.
        :param lookup_lines: If True, lookup lines for each frame immediately,
            otherwise lookup is deferred until the frame is rendered.
        :param capture_locals: If True, the local variables from each frame will
            be captured as object representations into the FrameSummary.
        """

    @classmethod
    def from_list(cls, a_list: Iterable[FrameSummary | _FrameSummaryTuple]) -> StackSummary:
        """
        Create a StackSummary object from a supplied list of
        FrameSummary objects or old-style list of tuples.
        """
    if sys.version_info >= (3, 11):
        def format_frame_summary(self, frame_summary: FrameSummary) -> str:
            """Format the lines for a single FrameSummary.

            Returns a string representing one frame involved in the stack. This
            gets called for every frame to be printed in the stack summary.
            """

    def format(self) -> list[str]:
        """Format the stack ready for printing.

        Returns a list of strings ready for printing.  Each string in the
        resulting list corresponds to a single frame from the stack.
        Each string ends in a newline; the strings may contain internal
        newlines as well, for those items with source text lines.

        For long sequences of the same frame and line, the first few
        repetitions are shown, followed by a summary line stating the exact
        number of further repetitions.
        """
