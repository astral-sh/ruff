from _typeshed import FileDescriptorOrPath
from typing import Final, Literal, Protocol, overload
from xml.etree.ElementTree import Element

class _Loader(Protocol):
    @overload
    def __call__(self, href: FileDescriptorOrPath, parse: Literal["xml"], encoding: str | None = None) -> Element: ...
    @overload
    def __call__(self, href: FileDescriptorOrPath, parse: Literal["text"], encoding: str | None = None) -> str: ...

XINCLUDE: Final[str]
XINCLUDE_INCLUDE: Final[str]
XINCLUDE_FALLBACK: Final[str]

DEFAULT_MAX_INCLUSION_DEPTH: Final = 6

class FatalIncludeError(SyntaxError): ...

@overload
def default_loader(href: FileDescriptorOrPath, parse: Literal["xml"], encoding: str | None = None) -> Element: ...
@overload
def default_loader(href: FileDescriptorOrPath, parse: Literal["text"], encoding: str | None = None) -> str: ...
def include(elem: Element, loader: _Loader | None = None, base_url: str | None = None, max_depth: int | None = 6) -> None: ...

class LimitedRecursiveIncludeError(FatalIncludeError): ...
