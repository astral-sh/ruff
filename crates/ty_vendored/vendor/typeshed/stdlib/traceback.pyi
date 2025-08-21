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

def print_tb(tb: TracebackType | None, limit: int | None = None, file: SupportsWrite[str] | None = None) -> None: ...

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
    ) -> None: ...
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
    ) -> list[str]: ...
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
    ) -> None: ...
    def format_exception(
        etype: type[BaseException] | None,
        value: BaseException | None,
        tb: TracebackType | None,
        limit: int | None = None,
        chain: bool = True,
    ) -> list[str]: ...

def print_exc(limit: int | None = None, file: SupportsWrite[str] | None = None, chain: bool = True) -> None: ...
def print_last(limit: int | None = None, file: SupportsWrite[str] | None = None, chain: bool = True) -> None: ...
def print_stack(f: FrameType | None = None, limit: int | None = None, file: SupportsWrite[str] | None = None) -> None: ...
def extract_tb(tb: TracebackType | None, limit: int | None = None) -> StackSummary: ...
def extract_stack(f: FrameType | None = None, limit: int | None = None) -> StackSummary: ...
def format_list(extracted_list: Iterable[FrameSummary | _FrameSummaryTuple]) -> list[str]: ...
def print_list(extracted_list: Iterable[FrameSummary | _FrameSummaryTuple], file: SupportsWrite[str] | None = None) -> None: ...

if sys.version_info >= (3, 13):
    @overload
    def format_exception_only(exc: BaseException | None, /, *, show_group: bool = False) -> list[str]: ...
    @overload
    def format_exception_only(exc: Unused, /, value: BaseException | None, *, show_group: bool = False) -> list[str]: ...

elif sys.version_info >= (3, 10):
    @overload
    def format_exception_only(exc: BaseException | None, /) -> list[str]: ...
    @overload
    def format_exception_only(exc: Unused, /, value: BaseException | None) -> list[str]: ...

else:
    def format_exception_only(etype: type[BaseException] | None, value: BaseException | None) -> list[str]: ...

def format_exc(limit: int | None = None, chain: bool = True) -> str: ...
def format_tb(tb: TracebackType | None, limit: int | None = None) -> list[str]: ...
def format_stack(f: FrameType | None = None, limit: int | None = None) -> list[str]: ...
def clear_frames(tb: TracebackType | None) -> None: ...
def walk_stack(f: FrameType | None) -> Iterator[tuple[FrameType, int]]: ...
def walk_tb(tb: TracebackType | None) -> Iterator[tuple[FrameType, int]]: ...

if sys.version_info >= (3, 11):
    class _ExceptionPrintContext:
        def indent(self) -> str: ...
        def emit(self, text_gen: str | Iterable[str], margin_char: str | None = None) -> Generator[str, None, None]: ...

class TracebackException:
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
        ) -> Self: ...
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
        ) -> Self: ...
    else:
        @classmethod
        def from_exception(
            cls, exc: BaseException, *, limit: int | None = None, lookup_lines: bool = True, capture_locals: bool = False
        ) -> Self: ...

    def __eq__(self, other: object) -> bool: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]
    if sys.version_info >= (3, 11):
        def format(self, *, chain: bool = True, _ctx: _ExceptionPrintContext | None = None) -> Generator[str, None, None]: ...
    else:
        def format(self, *, chain: bool = True) -> Generator[str, None, None]: ...

    if sys.version_info >= (3, 13):
        def format_exception_only(self, *, show_group: bool = False, _depth: int = 0) -> Generator[str, None, None]: ...
    else:
        def format_exception_only(self) -> Generator[str, None, None]: ...

    if sys.version_info >= (3, 11):
        def print(self, *, file: SupportsWrite[str] | None = None, chain: bool = True) -> None: ...

class FrameSummary:
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
        ) -> None: ...
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
        ) -> None: ...
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
    @classmethod
    def extract(
        cls,
        frame_gen: Iterable[tuple[FrameType, int]],
        *,
        limit: int | None = None,
        lookup_lines: bool = True,
        capture_locals: bool = False,
    ) -> StackSummary: ...
    @classmethod
    def from_list(cls, a_list: Iterable[FrameSummary | _FrameSummaryTuple]) -> StackSummary: ...
    if sys.version_info >= (3, 11):
        def format_frame_summary(self, frame_summary: FrameSummary) -> str: ...

    def format(self) -> list[str]: ...
