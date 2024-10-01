from abc import ABCMeta, abstractmethod
from collections.abc import Callable
from email.errors import MessageDefect
from email.header import Header
from email.message import Message
from typing_extensions import Self

class _PolicyBase:
    def __init__(
        self,
        *,
        max_line_length: int | None = 78,
        linesep: str = "\n",
        cte_type: str = "8bit",
        raise_on_defect: bool = False,
        mangle_from_: bool = ...,  # default depends on sub-class
        message_factory: Callable[[Policy], Message] | None = None,
        # Added in Python 3.8.20, 3.9.20, 3.10.15, 3.11.10, 3.12.5
        verify_generated_headers: bool = True,
    ) -> None: ...
    def clone(
        self,
        *,
        max_line_length: int | None = ...,
        linesep: str = ...,
        cte_type: str = ...,
        raise_on_defect: bool = ...,
        mangle_from_: bool = ...,
        message_factory: Callable[[Policy], Message] | None = ...,
        # Added in Python 3.8.20, 3.9.20, 3.10.15, 3.11.10, 3.12.5
        verify_generated_headers: bool = ...,
    ) -> Self: ...
    def __add__(self, other: Policy) -> Self: ...

class Policy(_PolicyBase, metaclass=ABCMeta):
    max_line_length: int | None
    linesep: str
    cte_type: str
    raise_on_defect: bool
    mangle_from_: bool
    message_factory: Callable[[Policy], Message] | None
    # Added in Python 3.8.20, 3.9.20, 3.10.15, 3.11.10, 3.12.5
    verify_generated_headers: bool

    def handle_defect(self, obj: Message, defect: MessageDefect) -> None: ...
    def register_defect(self, obj: Message, defect: MessageDefect) -> None: ...
    def header_max_count(self, name: str) -> int | None: ...
    @abstractmethod
    def header_source_parse(self, sourcelines: list[str]) -> tuple[str, str]: ...
    @abstractmethod
    def header_store_parse(self, name: str, value: str) -> tuple[str, str]: ...
    @abstractmethod
    def header_fetch_parse(self, name: str, value: str) -> str: ...
    @abstractmethod
    def fold(self, name: str, value: str) -> str: ...
    @abstractmethod
    def fold_binary(self, name: str, value: str) -> bytes: ...

class Compat32(Policy):
    def header_source_parse(self, sourcelines: list[str]) -> tuple[str, str]: ...
    def header_store_parse(self, name: str, value: str) -> tuple[str, str]: ...
    def header_fetch_parse(self, name: str, value: str) -> str | Header: ...  # type: ignore[override]
    def fold(self, name: str, value: str) -> str: ...
    def fold_binary(self, name: str, value: str) -> bytes: ...

compat32: Compat32
