from abc import ABCMeta, abstractmethod
from email.errors import MessageDefect
from email.header import Header
from email.message import Message
from typing import Generic, Protocol, TypeVar, type_check_only
from typing_extensions import Self

_MessageT = TypeVar("_MessageT", bound=Message, default=Message)

@type_check_only
class _MessageFactory(Protocol[_MessageT]):
    def __call__(self, policy: Policy[_MessageT]) -> _MessageT: ...

# Policy below is the only known direct subclass of _PolicyBase. We therefore
# assume that the __init__ arguments and attributes of _PolicyBase are
# the same as those of Policy.
class _PolicyBase(Generic[_MessageT]):
    max_line_length: int | None
    linesep: str
    cte_type: str
    raise_on_defect: bool
    mangle_from_: bool
    message_factory: _MessageFactory[_MessageT] | None
    # Added in Python 3.8.20, 3.9.20, 3.10.15, 3.11.10, 3.12.5
    verify_generated_headers: bool

    def __init__(
        self,
        *,
        max_line_length: int | None = 78,
        linesep: str = "\n",
        cte_type: str = "8bit",
        raise_on_defect: bool = False,
        mangle_from_: bool = ...,  # default depends on sub-class
        message_factory: _MessageFactory[_MessageT] | None = None,
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
        message_factory: _MessageFactory[_MessageT] | None = ...,
        # Added in Python 3.8.20, 3.9.20, 3.10.15, 3.11.10, 3.12.5
        verify_generated_headers: bool = ...,
    ) -> Self: ...
    def __add__(self, other: Policy) -> Self: ...

class Policy(_PolicyBase[_MessageT], metaclass=ABCMeta):
    def handle_defect(self, obj: _MessageT, defect: MessageDefect) -> None: ...
    def register_defect(self, obj: _MessageT, defect: MessageDefect) -> None: ...
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

class Compat32(Policy[_MessageT]):
    def header_source_parse(self, sourcelines: list[str]) -> tuple[str, str]: ...
    def header_store_parse(self, name: str, value: str) -> tuple[str, str]: ...
    def header_fetch_parse(self, name: str, value: str) -> str | Header: ...  # type: ignore[override]
    def fold(self, name: str, value: str) -> str: ...
    def fold_binary(self, name: str, value: str) -> bytes: ...

compat32: Compat32[Message]
