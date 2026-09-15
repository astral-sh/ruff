from _typeshed import Incomplete
from collections.abc import Callable
from email._policybase import Compat32 as Compat32, Policy as Policy, _MessageFactory, _MessageT, compat32 as compat32
from email.contentmanager import ContentManager
from email.message import EmailMessage
from typing import overload
from typing_extensions import Self

__all__ = ["Compat32", "compat32", "Policy", "EmailPolicy", "default", "strict", "SMTP", "HTTP"]

class EmailPolicy(Policy[_MessageT]):
    utf8: bool
    refold_source: str
    header_factory: Callable[[str, Incomplete], Incomplete]
    content_manager: ContentManager

    @overload
    def __init__(
        self: EmailPolicy[EmailMessage],
        *,
        max_line_length: int | None = 78,
        linesep: str = "\n",
        cte_type: str = "8bit",
        raise_on_defect: bool = False,
        mangle_from_: bool = ...,  # default depends on sub-class
        message_factory: None = None,
        # Added in Python 3.9.20, 3.10.15, 3.11.10, 3.12.5
        verify_generated_headers: bool = True,
        utf8: bool = False,
        refold_source: str = "long",
        header_factory: Callable[[str, str], str] = ...,
        content_manager: ContentManager = ...,
    ) -> None: ...
    @overload
    def __init__(
        self,
        *,
        max_line_length: int | None = 78,
        linesep: str = "\n",
        cte_type: str = "8bit",
        raise_on_defect: bool = False,
        mangle_from_: bool = ...,  # default depends on sub-class
        message_factory: _MessageFactory[_MessageT],
        # Added in Python 3.9.20, 3.10.15, 3.11.10, 3.12.5
        verify_generated_headers: bool = True,
        utf8: bool = False,
        refold_source: str = "long",
        header_factory: Callable[[str, str], str] = ...,
        content_manager: ContentManager = ...,
    ) -> None: ...

    def header_source_parse(self, sourcelines: list[str]) -> tuple[str, str]: ...
    def header_store_parse(self, name: str, value) -> tuple[str, Incomplete]: ...
    def header_fetch_parse(self, name: str, value: str): ...
    def fold(self, name: str, value: str): ...
    def fold_binary(self, name: str, value: str) -> bytes: ...
    def clone(
        self,
        *,
        max_line_length: int | None = ...,
        linesep: str = ...,
        cte_type: str = ...,
        raise_on_defect: bool = ...,
        mangle_from_: bool = ...,
        message_factory: _MessageFactory[_MessageT] | None = ...,
        # Added in Python 3.9.20, 3.10.15, 3.11.10, 3.12.5
        verify_generated_headers: bool = ...,
        utf8: bool = ...,
        refold_source: str = ...,
        header_factory: Callable[[str, str], str] = ...,
        content_manager: ContentManager = ...,
    ) -> Self: ...

default: EmailPolicy[EmailMessage]
SMTP: EmailPolicy[EmailMessage]
SMTPUTF8: EmailPolicy[EmailMessage]
HTTP: EmailPolicy[EmailMessage]
strict: EmailPolicy[EmailMessage]
