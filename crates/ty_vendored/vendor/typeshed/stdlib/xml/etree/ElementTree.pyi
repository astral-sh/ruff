"""Lightweight XML support for Python.

XML is an inherently hierarchical data format, and the most natural way to
represent it is with a tree.  This module has two classes for this purpose:

   1. ElementTree represents the whole XML document as a tree and

   2. Element represents a single node in this tree.

Interactions with the whole document (reading and writing to/from files) are
usually done on the ElementTree level.  Interactions with a single XML element
and its sub-elements are done on the Element level.

Element is a flexible container object designed to store hierarchical data
structures in memory. It can be described as a cross between a list and a
dictionary.  Each Element has a number of properties associated with it:

   'tag' - a string containing the element's name.

   'attributes' - a Python dictionary storing the element's attributes.

   'text' - a string containing the element's text content.

   'tail' - an optional string containing text after the element's end tag.

   And a number of child elements stored in a Python sequence.

To create an element instance, use the Element constructor,
or the SubElement factory function.

You can also use the ElementTree class to wrap an element structure
and convert it to and from XML.

"""

import sys
from _collections_abc import dict_keys
from _typeshed import FileDescriptorOrPath, ReadableBuffer, SupportsRead, SupportsWrite
from collections.abc import Callable, Generator, ItemsView, Iterable, Iterator, Mapping, Sequence
from typing import Any, Final, Generic, Literal, Protocol, SupportsIndex, TypeVar, overload, type_check_only
from typing_extensions import TypeAlias, TypeGuard, deprecated, disjoint_base
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
def iselement(element: object) -> TypeGuard[Element]:
    """Return True if *element* appears to be an Element."""

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
) -> str:
    """Convert XML to its C14N 2.0 serialised form.

    If *out* is provided, it must be a file or file-like object that receives
    the serialised canonical XML output (text, not bytes) through its ``.write()``
    method.  To write to a file, open it in text mode with encoding "utf-8".
    If *out* is not provided, this function returns the output as text string.

    Either *xml_data* (an XML string) or *from_file* (a file path or
    file-like object) must be provided as input.

    The configuration options are the same as for the ``C14NWriterTarget``.
    """

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
# functions defined in this module.
_ElementCallable: TypeAlias = Callable[..., Element[_ElementCallable]]

_Tag = TypeVar("_Tag", default=str, bound=str | _ElementCallable)
_OtherTag = TypeVar("_OtherTag", default=str, bound=str | _ElementCallable)

@disjoint_base
class Element(Generic[_Tag]):
    tag: _Tag
    attrib: dict[str, str]
    text: str | None
    tail: str | None
    def __init__(self, tag: _Tag, attrib: dict[str, str] = {}, **extra: str) -> None: ...
    def append(self, subelement: Element[Any], /) -> None: ...
    def clear(self) -> None: ...
    def extend(self, elements: Iterable[Element[Any]], /) -> None: ...
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
    def insert(self, index: int, subelement: Element[Any], /) -> None: ...
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
    def remove(self, subelement: Element[Any], /) -> None: ...
    def set(self, key: str, value: str, /) -> None: ...
    def __copy__(self) -> Element[_Tag]: ...  # returns the type of self in Python impl, but not in C impl
    def __deepcopy__(self, memo: Any, /) -> Element: ...  # Only exists in C impl
    def __delitem__(self, key: SupportsIndex | slice, /) -> None:
        """Delete self[key]."""

    @overload
    def __getitem__(self, key: SupportsIndex, /) -> Element:
        """Return self[key]."""

    @overload
    def __getitem__(self, key: slice, /) -> list[Element]: ...
    def __len__(self) -> int:
        """Return len(self)."""
    # Doesn't actually exist at runtime, but instance of the class are indeed iterable due to __getitem__.
    def __iter__(self) -> Iterator[Element]: ...
    @overload
    def __setitem__(self, key: SupportsIndex, value: Element[Any], /) -> None:
        """Set self[key] to value."""

    @overload
    def __setitem__(self, key: slice, value: Iterable[Element[Any]], /) -> None: ...

    # Doesn't really exist in earlier versions, where __len__ is called implicitly instead
    @deprecated("Testing an element's truth value is deprecated.")
    def __bool__(self) -> bool:
        """True if self else False"""

def SubElement(parent: Element[Any], tag: str, attrib: dict[str, str] = ..., **extra: str) -> Element: ...
def Comment(text: str | None = None) -> Element[_ElementCallable]:
    """Comment element factory.

    This function creates a special element which the standard serializer
    serializes as an XML comment.

    *text* is a string containing the comment string.

    """

def ProcessingInstruction(target: str, text: str | None = None) -> Element[_ElementCallable]:
    """Processing Instruction element factory.

    This function creates a special element which the standard serializer
    serializes as an XML comment.

    *target* is a string containing the processing instruction, *text* is a
    string containing the processing instruction contents, if any.

    """

PI = ProcessingInstruction

class QName:
    """Qualified name wrapper.

    This class can be used to wrap a QName attribute value in order to get
    proper namespace handing on output.

    *text_or_uri* is a string containing the QName value either in the form
    {uri}local, or if the tag argument is given, the URI part of a QName.

    *tag* is an optional argument which if given, will make the first
    argument (text_or_uri) be interpreted as a URI, and this argument (tag)
    be interpreted as a local name.

    """

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
    """An XML element hierarchy.

    This class also provides support for serialization to and from
    standard XML.

    *element* is an optional root element node,
    *file* is an optional file handle or file name of an XML file whose
    contents will be used to initialize the tree with.

    """

    def __init__(self, element: Element[Any] | None = None, file: _FileRead | None = None) -> None: ...
    def getroot(self) -> _Root:
        """Return root element of this tree."""

    def _setroot(self, element: Element[Any]) -> None:
        """Replace root element of this tree.

        This will discard the current contents of the tree and replace it
        with the given element.  Use with care!

        """

    def parse(self, source: _FileRead, parser: XMLParser | None = None) -> Element:
        """Load external XML document into element tree.

        *source* is a file name or file object, *parser* is an optional parser
        instance that defaults to XMLParser.

        ParseError is raised if the parser fails to parse the document.

        Returns the root element of the given source document.

        """

    def iter(self, tag: str | None = None) -> Generator[Element, None, None]:
        """Create and return tree iterator for the root element.

        The iterator loops over all elements in this tree, in document order.

        *tag* is a string with the tag name to iterate over
        (default is to return all elements).

        """

    def find(self, path: str, namespaces: dict[str, str] | None = None) -> Element | None:
        """Find first matching element by tag name or path.

        Same as getroot().find(path), which is Element.find()

        *path* is a string having either an element tag or an XPath,
        *namespaces* is an optional mapping from namespace prefix to full name.

        Return the first matching element, or None if no element was found.

        """

    @overload
    def findtext(self, path: str, default: None = None, namespaces: dict[str, str] | None = None) -> str | None:
        """Find first matching element by tag name or path.

        Same as getroot().findtext(path),  which is Element.findtext()

        *path* is a string having either an element tag or an XPath,
        *namespaces* is an optional mapping from namespace prefix to full name.

        Return the first matching element, or None if no element was found.

        """

    @overload
    def findtext(self, path: str, default: _T, namespaces: dict[str, str] | None = None) -> _T | str: ...
    def findall(self, path: str, namespaces: dict[str, str] | None = None) -> list[Element]:
        """Find all matching subelements by tag name or path.

        Same as getroot().findall(path), which is Element.findall().

        *path* is a string having either an element tag or an XPath,
        *namespaces* is an optional mapping from namespace prefix to full name.

        Return list containing all matching elements in document order.

        """

    @overload
    def iterfind(self, path: Literal[""], namespaces: dict[str, str] | None = None) -> None:  # type: ignore[overload-overlap]
        """Find all matching subelements by tag name or path.

        Same as getroot().iterfind(path), which is element.iterfind()

        *path* is a string having either an element tag or an XPath,
        *namespaces* is an optional mapping from namespace prefix to full name.

        Return an iterable yielding all matching elements in document order.

        """

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
    ) -> None:
        """Write element tree to a file as XML.

        Arguments:
          *file_or_filename* -- file name or a file object opened for writing

          *encoding* -- the output encoding (default: US-ASCII)

          *xml_declaration* -- bool indicating if an XML declaration should be
                               added to the output. If None, an XML declaration
                               is added if encoding IS NOT either of:
                               US-ASCII, UTF-8, or Unicode

          *default_namespace* -- sets the default XML namespace (for "xmlns")

          *method* -- either "xml" (default), "html, "text", or "c14n"

          *short_empty_elements* -- controls the formatting of elements
                                    that contain no content. If True (default)
                                    they are emitted as a single self-closed
                                    tag, otherwise they are emitted as a pair
                                    of start/end tags

        """

    def write_c14n(self, file: _FileWriteC14N) -> None: ...

HTML_EMPTY: Final[set[str]]

def register_namespace(prefix: str, uri: str) -> None:
    """Register a namespace prefix.

    The registry is global, and any existing mapping for either the
    given prefix or the namespace URI will be removed.

    *prefix* is the namespace prefix, *uri* is a namespace uri. Tags and
    attributes in this namespace will be serialized with prefix if possible.

    ValueError is raised if prefix is reserved or is invalid.

    """

@overload
def tostring(
    element: Element[Any],
    encoding: None = None,
    method: Literal["xml", "html", "text", "c14n"] | None = None,
    *,
    xml_declaration: bool | None = None,
    default_namespace: str | None = None,
    short_empty_elements: bool = True,
) -> bytes:
    """Generate string representation of XML element.

    All subelements are included.  If encoding is "unicode", a string
    is returned. Otherwise a bytestring is returned.

    *element* is an Element instance, *encoding* is an optional output
    encoding defaulting to US-ASCII, *method* is an optional output which can
    be one of "xml" (default), "html", "text" or "c14n", *default_namespace*
    sets the default XML namespace (for "xmlns").

    Returns an (optionally) encoded string containing the XML data.

    """

@overload
def tostring(
    element: Element[Any],
    encoding: Literal["unicode"],
    method: Literal["xml", "html", "text", "c14n"] | None = None,
    *,
    xml_declaration: bool | None = None,
    default_namespace: str | None = None,
    short_empty_elements: bool = True,
) -> str: ...
@overload
def tostring(
    element: Element[Any],
    encoding: str,
    method: Literal["xml", "html", "text", "c14n"] | None = None,
    *,
    xml_declaration: bool | None = None,
    default_namespace: str | None = None,
    short_empty_elements: bool = True,
) -> Any: ...
@overload
def tostringlist(
    element: Element[Any],
    encoding: None = None,
    method: Literal["xml", "html", "text", "c14n"] | None = None,
    *,
    xml_declaration: bool | None = None,
    default_namespace: str | None = None,
    short_empty_elements: bool = True,
) -> list[bytes]: ...
@overload
def tostringlist(
    element: Element[Any],
    encoding: Literal["unicode"],
    method: Literal["xml", "html", "text", "c14n"] | None = None,
    *,
    xml_declaration: bool | None = None,
    default_namespace: str | None = None,
    short_empty_elements: bool = True,
) -> list[str]: ...
@overload
def tostringlist(
    element: Element[Any],
    encoding: str,
    method: Literal["xml", "html", "text", "c14n"] | None = None,
    *,
    xml_declaration: bool | None = None,
    default_namespace: str | None = None,
    short_empty_elements: bool = True,
) -> list[Any]: ...
def dump(elem: Element[Any] | ElementTree[Any]) -> None:
    """Write element tree or element structure to sys.stdout.

    This function should be used for debugging only.

    *elem* is either an ElementTree, or a single Element.  The exact output
    format is implementation dependent.  In this version, it's written as an
    ordinary XML file.

    """

def indent(tree: Element[Any] | ElementTree[Any], space: str = "  ", level: int = 0) -> None:
    """Indent an XML document by inserting newlines and indentation space
    after elements.

    *tree* is the ElementTree or Element to modify.  The (root) element
    itself will not be changed, but the tail text of all elements in its
    subtree will be adapted.

    *space* is the whitespace to insert for each indentation level, two
    space characters by default.

    *level* is the initial indentation level. Setting this to a higher
    value than 0 can be used for indenting subtrees that are more deeply
    nested inside of a document.
    """

def parse(source: _FileRead, parser: XMLParser[Any] | None = None) -> ElementTree[Element]:
    """Parse XML document into element tree.

    *source* is a filename or file object containing XML data,
    *parser* is an optional parser instance defaulting to XMLParser.

    Return an ElementTree instance.

    """

# This class is defined inside the body of iterparse
@type_check_only
class _IterParseIterator(Iterator[tuple[str, Element]], Protocol):
    def __next__(self) -> tuple[str, Element]: ...
    if sys.version_info >= (3, 13):
        def close(self) -> None: ...
    if sys.version_info >= (3, 11):
        def __del__(self) -> None: ...

def iterparse(source: _FileRead, events: Sequence[str] | None = None, parser: XMLParser | None = None) -> _IterParseIterator:
    """Incrementally parse XML document into ElementTree.

    This class also reports what's going on to the user based on the
    *events* it is initialized with.  The supported events are the strings
    "start", "end", "start-ns" and "end-ns" (the "ns" events are used to get
    detailed namespace information).  If *events* is omitted, only
    "end" events are reported.

    *source* is a filename or file object containing XML data, *events* is
    a list of events to report back, *parser* is an optional parser instance.

    Returns an iterator providing (event, elem) pairs.

    """

_EventQueue: TypeAlias = tuple[str] | tuple[str, tuple[str, str]] | tuple[str, None]

class XMLPullParser(Generic[_E]):
    def __init__(self, events: Sequence[str] | None = None, *, _parser: XMLParser[_E] | None = None) -> None: ...
    def feed(self, data: str | ReadableBuffer) -> None:
        """Feed encoded data to parser."""

    def close(self) -> None:
        """Finish feeding data to parser.

        Unlike XMLParser, does not return the root element. Use
        read_events() to consume elements from XMLPullParser.
        """

    def read_events(self) -> Iterator[_EventQueue | tuple[str, _E]]:
        """Return an iterator over currently available (event, elem) pairs.

        Events are consumed from the internal event queue as they are
        retrieved from the iterator.
        """

    def flush(self) -> None: ...

def XML(text: str | ReadableBuffer, parser: XMLParser | None = None) -> Element:
    """Parse XML document from string constant.

    This function can be used to embed "XML Literals" in Python code.

    *text* is a string containing XML data, *parser* is an
    optional parser instance, defaulting to the standard XMLParser.

    Returns an Element instance.

    """

def XMLID(text: str | ReadableBuffer, parser: XMLParser | None = None) -> tuple[Element, dict[str, Element]]:
    """Parse XML document from string constant for its IDs.

    *text* is a string containing XML data, *parser* is an
    optional parser instance, defaulting to the standard XMLParser.

    Returns an (Element, dict) tuple, in which the
    dict maps element id:s to elements.

    """

# This is aliased to XML in the source.
fromstring = XML

def fromstringlist(sequence: Sequence[str | ReadableBuffer], parser: XMLParser | None = None) -> Element:
    """Parse XML document from sequence of string fragments.

    *sequence* is a list of other sequence, *parser* is an optional parser
    instance, defaulting to the standard XMLParser.

    Returns an Element instance.

    """

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

@disjoint_base
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
    """
    Canonicalization writer target for the XMLParser.

    Serialises parse events to XML C14N 2.0.

    The *write* function is used for writing out the resulting data stream
    as text (not bytes).  To write to a file, open it in text mode with encoding
    "utf-8" and pass its ``.write`` method.

    Configuration options:

    - *with_comments*: set to true to include comments
    - *strip_text*: set to true to strip whitespace before and after text content
    - *rewrite_prefixes*: set to true to replace namespace prefixes by "n{number}"
    - *qname_aware_tags*: a set of qname aware tag names in which prefixes
                          should be replaced in text content
    - *qname_aware_attrs*: a set of qname aware attribute names in which prefixes
                           should be replaced in text content
    - *exclude_attrs*: a set of attribute names that should not be serialised
    - *exclude_tags*: a set of tag names that should not be serialised
    """

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
@disjoint_base
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
