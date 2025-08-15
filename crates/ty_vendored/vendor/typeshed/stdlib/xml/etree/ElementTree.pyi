import sys
from _collections_abc import dict_keys
from _typeshed import FileDescriptorOrPath, ReadableBuffer, SupportsRead, SupportsWrite
from collections.abc import Callable, Generator, ItemsView, Iterable, Iterator, Mapping, Sequence
from typing import Any, Final, Generic, Literal, Protocol, SupportsIndex, TypeVar, overload, type_check_only
from typing_extensions import TypeAlias, TypeGuard, deprecated
from xml.parsers.expat import XMLParserType

__all__ = [
    "C14NWriterTarget",
    "Comment",
    "dump",
    "Element",
    "ElementTree",
    "canonicalize",
    "fromstring",
    "fromstringlist",
    "indent",
    "iselement",
    "iterparse",
    "parse",
    "ParseError",
    "PI",
    "ProcessingInstruction",
    "QName",
    "SubElement",
    "tostring",
    "tostringlist",
    "TreeBuilder",
    "VERSION",
    "XML",
    "XMLID",
    "XMLParser",
    "XMLPullParser",
    "register_namespace",
]

_T = TypeVar("_T")
_FileRead: TypeAlias = FileDescriptorOrPath | SupportsRead[bytes] | SupportsRead[str]
_FileWriteC14N: TypeAlias = FileDescriptorOrPath | SupportsWrite[bytes]
_FileWrite: TypeAlias = _FileWriteC14N | SupportsWrite[str]

VERSION: Final[str]

class ParseError(SyntaxError):
    code: int
    position: tuple[int, int]

# In reality it works based on `.tag` attribute duck typing.
def iselement(element: object) -> TypeGuard[Element]: ...
@overload
def canonicalize(
    xml_data: str | ReadableBuffer | None = None,
    *,
    out: None = None,
    from_file: _FileRead | None = None,
    with_comments: bool = False,
    strip_text: bool = False,
    rewrite_prefixes: bool = False,
    qname_aware_tags: Iterable[str] | None = None,
    qname_aware_attrs: Iterable[str] | None = None,
    exclude_attrs: Iterable[str] | None = None,
    exclude_tags: Iterable[str] | None = None,
) -> str: ...
@overload
def canonicalize(
    xml_data: str | ReadableBuffer | None = None,
    *,
    out: SupportsWrite[str],
    from_file: _FileRead | None = None,
    with_comments: bool = False,
    strip_text: bool = False,
    rewrite_prefixes: bool = False,
    qname_aware_tags: Iterable[str] | None = None,
    qname_aware_attrs: Iterable[str] | None = None,
    exclude_attrs: Iterable[str] | None = None,
    exclude_tags: Iterable[str] | None = None,
) -> None: ...

# The tag for Element can be set to the Comment or ProcessingInstruction
# functions defined in this module. _ElementCallable could be a recursive
# type, but defining it that way uncovered a bug in pytype.
_ElementCallable: TypeAlias = Callable[..., Element[Any]]
_CallableElement: TypeAlias = Element[_ElementCallable]

_Tag = TypeVar("_Tag", default=str, bound=str | _ElementCallable)
_OtherTag = TypeVar("_OtherTag", default=str, bound=str | _ElementCallable)

class Element(Generic[_Tag]):
    tag: _Tag
    attrib: dict[str, str]
    text: str | None
    tail: str | None
    def __init__(self, tag: _Tag, attrib: dict[str, str] = {}, **extra: str) -> None: ...
    def append(self, subelement: Element[Any], /) -> None: ...
    def clear(self) -> None: ...
    def extend(self, elements: Iterable[Element], /) -> None: ...
    def find(self, path: str, namespaces: dict[str, str] | None = None) -> Element | None: ...
    def findall(self, path: str, namespaces: dict[str, str] | None = None) -> list[Element]: ...
    @overload
    def findtext(self, path: str, default: None = None, namespaces: dict[str, str] | None = None) -> str | None: ...
    @overload
    def findtext(self, path: str, default: _T, namespaces: dict[str, str] | None = None) -> _T | str: ...
    @overload
    def get(self, key: str, default: None = None) -> str | None: ...
    @overload
    def get(self, key: str, default: _T) -> str | _T: ...
    def insert(self, index: int, subelement: Element, /) -> None: ...
    def items(self) -> ItemsView[str, str]: ...
    def iter(self, tag: str | None = None) -> Generator[Element, None, None]: ...
    @overload
    def iterfind(self, path: Literal[""], namespaces: dict[str, str] | None = None) -> None: ...  # type: ignore[overload-overlap]
    @overload
    def iterfind(self, path: str, namespaces: dict[str, str] | None = None) -> Generator[Element, None, None]: ...
    def itertext(self) -> Generator[str, None, None]: ...
    def keys(self) -> dict_keys[str, str]: ...
    # makeelement returns the type of self in Python impl, but not in C impl
    def makeelement(self, tag: _OtherTag, attrib: dict[str, str], /) -> Element[_OtherTag]: ...
    def remove(self, subelement: Element, /) -> None: ...
    def set(self, key: str, value: str, /) -> None: ...
    def __copy__(self) -> Element[_Tag]: ...  # returns the type of self in Python impl, but not in C impl
    def __deepcopy__(self, memo: Any, /) -> Element: ...  # Only exists in C impl
    def __delitem__(self, key: SupportsIndex | slice, /) -> None: ...
    @overload
    def __getitem__(self, key: SupportsIndex, /) -> Element: ...
    @overload
    def __getitem__(self, key: slice, /) -> list[Element]: ...
    def __len__(self) -> int: ...
    # Doesn't actually exist at runtime, but instance of the class are indeed iterable due to __getitem__.
    def __iter__(self) -> Iterator[Element]: ...
    @overload
    def __setitem__(self, key: SupportsIndex, value: Element, /) -> None: ...
    @overload
    def __setitem__(self, key: slice, value: Iterable[Element], /) -> None: ...

    # Doesn't really exist in earlier versions, where __len__ is called implicitly instead
    @deprecated("Testing an element's truth value is deprecated.")
    def __bool__(self) -> bool: ...

def SubElement(parent: Element, tag: str, attrib: dict[str, str] = ..., **extra: str) -> Element: ...
def Comment(text: str | None = None) -> _CallableElement: ...
def ProcessingInstruction(target: str, text: str | None = None) -> _CallableElement: ...

PI = ProcessingInstruction

class QName:
    text: str
    def __init__(self, text_or_uri: str, tag: str | None = None) -> None: ...
    def __lt__(self, other: QName | str) -> bool: ...
    def __le__(self, other: QName | str) -> bool: ...
    def __gt__(self, other: QName | str) -> bool: ...
    def __ge__(self, other: QName | str) -> bool: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

_Root = TypeVar("_Root", Element, Element | None, default=Element | None)

class ElementTree(Generic[_Root]):
    def __init__(self, element: Element | None = None, file: _FileRead | None = None) -> None: ...
    def getroot(self) -> _Root: ...
    def parse(self, source: _FileRead, parser: XMLParser | None = None) -> Element: ...
    def iter(self, tag: str | None = None) -> Generator[Element, None, None]: ...
    def find(self, path: str, namespaces: dict[str, str] | None = None) -> Element | None: ...
    @overload
    def findtext(self, path: str, default: None = None, namespaces: dict[str, str] | None = None) -> str | None: ...
    @overload
    def findtext(self, path: str, default: _T, namespaces: dict[str, str] | None = None) -> _T | str: ...
    def findall(self, path: str, namespaces: dict[str, str] | None = None) -> list[Element]: ...
    @overload
    def iterfind(self, path: Literal[""], namespaces: dict[str, str] | None = None) -> None: ...  # type: ignore[overload-overlap]
    @overload
    def iterfind(self, path: str, namespaces: dict[str, str] | None = None) -> Generator[Element, None, None]: ...
    def write(
        self,
        file_or_filename: _FileWrite,
        encoding: str | None = None,
        xml_declaration: bool | None = None,
        default_namespace: str | None = None,
        method: Literal["xml", "html", "text", "c14n"] | None = None,
        *,
        short_empty_elements: bool = True,
    ) -> None: ...
    def write_c14n(self, file: _FileWriteC14N) -> None: ...

HTML_EMPTY: set[str]

def register_namespace(prefix: str, uri: str) -> None: ...
@overload
def tostring(
    element: Element,
    encoding: None = None,
    method: Literal["xml", "html", "text", "c14n"] | None = None,
    *,
    xml_declaration: bool | None = None,
    default_namespace: str | None = None,
    short_empty_elements: bool = True,
) -> bytes: ...
@overload
def tostring(
    element: Element,
    encoding: Literal["unicode"],
    method: Literal["xml", "html", "text", "c14n"] | None = None,
    *,
    xml_declaration: bool | None = None,
    default_namespace: str | None = None,
    short_empty_elements: bool = True,
) -> str: ...
@overload
def tostring(
    element: Element,
    encoding: str,
    method: Literal["xml", "html", "text", "c14n"] | None = None,
    *,
    xml_declaration: bool | None = None,
    default_namespace: str | None = None,
    short_empty_elements: bool = True,
) -> Any: ...
@overload
def tostringlist(
    element: Element,
    encoding: None = None,
    method: Literal["xml", "html", "text", "c14n"] | None = None,
    *,
    xml_declaration: bool | None = None,
    default_namespace: str | None = None,
    short_empty_elements: bool = True,
) -> list[bytes]: ...
@overload
def tostringlist(
    element: Element,
    encoding: Literal["unicode"],
    method: Literal["xml", "html", "text", "c14n"] | None = None,
    *,
    xml_declaration: bool | None = None,
    default_namespace: str | None = None,
    short_empty_elements: bool = True,
) -> list[str]: ...
@overload
def tostringlist(
    element: Element,
    encoding: str,
    method: Literal["xml", "html", "text", "c14n"] | None = None,
    *,
    xml_declaration: bool | None = None,
    default_namespace: str | None = None,
    short_empty_elements: bool = True,
) -> list[Any]: ...
def dump(elem: Element | ElementTree[Any]) -> None: ...
def indent(tree: Element | ElementTree[Any], space: str = "  ", level: int = 0) -> None: ...
def parse(source: _FileRead, parser: XMLParser[Any] | None = None) -> ElementTree[Element]: ...

# This class is defined inside the body of iterparse
@type_check_only
class _IterParseIterator(Iterator[tuple[str, Element]], Protocol):
    def __next__(self) -> tuple[str, Element]: ...
    if sys.version_info >= (3, 13):
        def close(self) -> None: ...
    if sys.version_info >= (3, 11):
        def __del__(self) -> None: ...

def iterparse(source: _FileRead, events: Sequence[str] | None = None, parser: XMLParser | None = None) -> _IterParseIterator: ...

_EventQueue: TypeAlias = tuple[str] | tuple[str, tuple[str, str]] | tuple[str, None]

class XMLPullParser(Generic[_E]):
    def __init__(self, events: Sequence[str] | None = None, *, _parser: XMLParser[_E] | None = None) -> None: ...
    def feed(self, data: str | ReadableBuffer) -> None: ...
    def close(self) -> None: ...
    def read_events(self) -> Iterator[_EventQueue | tuple[str, _E]]: ...
    def flush(self) -> None: ...

def XML(text: str | ReadableBuffer, parser: XMLParser | None = None) -> Element: ...
def XMLID(text: str | ReadableBuffer, parser: XMLParser | None = None) -> tuple[Element, dict[str, Element]]: ...

# This is aliased to XML in the source.
fromstring = XML

def fromstringlist(sequence: Sequence[str | ReadableBuffer], parser: XMLParser | None = None) -> Element: ...

# This type is both not precise enough and too precise. The TreeBuilder
# requires the elementfactory to accept tag and attrs in its args and produce
# some kind of object that has .text and .tail properties.
# I've chosen to constrain the ElementFactory to always produce an Element
# because that is how almost everyone will use it.
# Unfortunately, the type of the factory arguments is dependent on how
# TreeBuilder is called by client code (they could pass strs, bytes or whatever);
# but we don't want to use a too-broad type, or it would be too hard to write
# elementfactories.
_ElementFactory: TypeAlias = Callable[[Any, dict[Any, Any]], Element]

class TreeBuilder:
    # comment_factory can take None because passing None to Comment is not an error
    def __init__(
        self,
        element_factory: _ElementFactory | None = None,
        *,
        comment_factory: Callable[[str | None], Element[Any]] | None = None,
        pi_factory: Callable[[str, str | None], Element[Any]] | None = None,
        insert_comments: bool = False,
        insert_pis: bool = False,
    ) -> None: ...
    insert_comments: bool
    insert_pis: bool

    def close(self) -> Element: ...
    def data(self, data: str, /) -> None: ...
    # tag and attrs are passed to the element_factory, so they could be anything
    # depending on what the particular factory supports.
    def start(self, tag: Any, attrs: dict[Any, Any], /) -> Element: ...
    def end(self, tag: str, /) -> Element: ...
    # These two methods have pos-only parameters in the C implementation
    def comment(self, text: str | None, /) -> Element[Any]: ...
    def pi(self, target: str, text: str | None = None, /) -> Element[Any]: ...

class C14NWriterTarget:
    def __init__(
        self,
        write: Callable[[str], object],
        *,
        with_comments: bool = False,
        strip_text: bool = False,
        rewrite_prefixes: bool = False,
        qname_aware_tags: Iterable[str] | None = None,
        qname_aware_attrs: Iterable[str] | None = None,
        exclude_attrs: Iterable[str] | None = None,
        exclude_tags: Iterable[str] | None = None,
    ) -> None: ...
    def data(self, data: str) -> None: ...
    def start_ns(self, prefix: str, uri: str) -> None: ...
    def start(self, tag: str, attrs: Mapping[str, str]) -> None: ...
    def end(self, tag: str) -> None: ...
    def comment(self, text: str) -> None: ...
    def pi(self, target: str, data: str) -> None: ...

# The target type is tricky, because the implementation doesn't
# require any particular attribute to be present. This documents the attributes
# that can be present, but uncommenting any of them would require them.
@type_check_only
class _Target(Protocol):
    # start: Callable[str, dict[str, str], Any] | None
    # end: Callable[[str], Any] | None
    # start_ns: Callable[[str, str], Any] | None
    # end_ns: Callable[[str], Any] | None
    # data: Callable[[str], Any] | None
    # comment: Callable[[str], Any]
    # pi: Callable[[str, str], Any] | None
    # close: Callable[[], Any] | None
    ...

_E = TypeVar("_E", default=Element)

# This is generic because the return type of close() depends on the target.
# The default target is TreeBuilder, which returns Element.
# C14NWriterTarget does not implement a close method, so using it results
# in a type of XMLParser[None].
class XMLParser(Generic[_E]):
    parser: XMLParserType
    target: _Target
    # TODO: what is entity used for???
    entity: dict[str, str]
    version: str
    def __init__(self, *, target: _Target | None = None, encoding: str | None = None) -> None: ...
    def close(self) -> _E: ...
    def feed(self, data: str | ReadableBuffer, /) -> None: ...
    def flush(self) -> None: ...
