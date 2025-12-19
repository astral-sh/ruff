"""Facility to use the Expat parser to load a minidom instance
from a string or file.

This avoids all the overhead of SAX and pulldom to gain performance.
"""

from _typeshed import ReadableBuffer, SupportsRead
from typing import Any, Final, NoReturn
from typing_extensions import TypeAlias
from xml.dom.minidom import Document, DocumentFragment, DOMImplementation, Element, Node, TypeInfo
from xml.dom.xmlbuilder import DOMBuilderFilter, Options
from xml.parsers.expat import XMLParserType

_Model: TypeAlias = tuple[int, int, str | None, tuple[Any, ...]]  # same as in pyexpat

TEXT_NODE: Final = Node.TEXT_NODE
CDATA_SECTION_NODE: Final = Node.CDATA_SECTION_NODE
DOCUMENT_NODE: Final = Node.DOCUMENT_NODE
FILTER_ACCEPT: Final = DOMBuilderFilter.FILTER_ACCEPT
FILTER_REJECT: Final = DOMBuilderFilter.FILTER_REJECT
FILTER_SKIP: Final = DOMBuilderFilter.FILTER_SKIP
FILTER_INTERRUPT: Final = DOMBuilderFilter.FILTER_INTERRUPT
theDOMImplementation: DOMImplementation

class ElementInfo:
    __slots__ = ("_attr_info", "_model", "tagName")
    tagName: str
    def __init__(self, tagName: str, model: _Model | None = None) -> None: ...
    def getAttributeType(self, aname: str) -> TypeInfo: ...
    def getAttributeTypeNS(self, namespaceURI: str | None, localName: str) -> TypeInfo: ...
    def isElementContent(self) -> bool: ...
    def isEmpty(self) -> bool: ...
    def isId(self, aname: str) -> bool: ...
    def isIdNS(self, euri: str, ename: str, auri: str, aname: str) -> bool: ...

class ExpatBuilder:
    """Document builder that uses Expat to build a ParsedXML.DOM document
    instance.
    """

    document: Document  # Created in self.reset()
    curNode: DocumentFragment | Element | Document  # Created in self.reset()
    def __init__(self, options: Options | None = None) -> None: ...
    def createParser(self) -> XMLParserType:
        """Create a new parser object."""

    def getParser(self) -> XMLParserType:
        """Return the parser object, creating a new one if needed."""

    def reset(self) -> None:
        """Free all data structures used during DOM construction."""

    def install(self, parser: XMLParserType) -> None:
        """Install the callbacks needed to build the DOM into the parser."""

    def parseFile(self, file: SupportsRead[ReadableBuffer | str]) -> Document:
        """Parse a document from a file object, returning the document
        node.
        """

    def parseString(self, string: str | ReadableBuffer) -> Document:
        """Parse a document from a string, returning the document node."""

    def start_doctype_decl_handler(
        self, doctypeName: str, systemId: str | None, publicId: str | None, has_internal_subset: bool
    ) -> None: ...
    def end_doctype_decl_handler(self) -> None: ...
    def pi_handler(self, target: str, data: str) -> None: ...
    def character_data_handler_cdata(self, data: str) -> None: ...
    def character_data_handler(self, data: str) -> None: ...
    def start_cdata_section_handler(self) -> None: ...
    def end_cdata_section_handler(self) -> None: ...
    def entity_decl_handler(
        self,
        entityName: str,
        is_parameter_entity: bool,
        value: str | None,
        base: str | None,
        systemId: str,
        publicId: str | None,
        notationName: str | None,
    ) -> None: ...
    def notation_decl_handler(self, notationName: str, base: str | None, systemId: str, publicId: str | None) -> None: ...
    def comment_handler(self, data: str) -> None: ...
    def external_entity_ref_handler(self, context: str, base: str | None, systemId: str | None, publicId: str | None) -> int: ...
    def first_element_handler(self, name: str, attributes: list[str]) -> None: ...
    def start_element_handler(self, name: str, attributes: list[str]) -> None: ...
    def end_element_handler(self, name: str) -> None: ...
    def element_decl_handler(self, name: str, model: _Model) -> None: ...
    def attlist_decl_handler(self, elem: str, name: str, type: str, default: str | None, required: bool) -> None: ...
    def xml_decl_handler(self, version: str, encoding: str | None, standalone: int) -> None: ...

class FilterVisibilityController:
    """Wrapper around a DOMBuilderFilter which implements the checks
    to make the whatToShow filter attribute work.
    """

    __slots__ = ("filter",)
    filter: DOMBuilderFilter
    def __init__(self, filter: DOMBuilderFilter) -> None: ...
    def startContainer(self, node: Node) -> int: ...
    def acceptNode(self, node: Node) -> int: ...

class FilterCrutch:
    __slots__ = ("_builder", "_level", "_old_start", "_old_end")
    def __init__(self, builder: ExpatBuilder) -> None: ...

class Rejecter(FilterCrutch):
    __slots__ = ()
    def start_element_handler(self, *args: Any) -> None: ...
    def end_element_handler(self, *args: Any) -> None: ...

class Skipper(FilterCrutch):
    __slots__ = ()
    def start_element_handler(self, *args: Any) -> None: ...
    def end_element_handler(self, *args: Any) -> None: ...

class FragmentBuilder(ExpatBuilder):
    """Builder which constructs document fragments given XML source
    text and a context node.

    The context node is expected to provide information about the
    namespace declarations which are in scope at the start of the
    fragment.
    """

    fragment: DocumentFragment | None
    originalDocument: Document
    context: Node
    def __init__(self, context: Node, options: Options | None = None) -> None: ...
    def reset(self) -> None: ...
    def parseFile(self, file: SupportsRead[ReadableBuffer | str]) -> DocumentFragment:  # type: ignore[override]
        """Parse a document fragment from a file object, returning the
        fragment node.
        """

    def parseString(self, string: ReadableBuffer | str) -> DocumentFragment:  # type: ignore[override]
        """Parse a document fragment from a string, returning the
        fragment node.
        """

    def external_entity_ref_handler(self, context: str, base: str | None, systemId: str | None, publicId: str | None) -> int: ...

class Namespaces:
    """Mix-in class for builders; adds support for namespaces."""

    def createParser(self) -> XMLParserType:
        """Create a new namespace-handling parser."""

    def install(self, parser: XMLParserType) -> None:
        """Insert the namespace-handlers onto the parser."""

    def start_namespace_decl_handler(self, prefix: str | None, uri: str) -> None:
        """Push this namespace declaration on our storage."""

    def start_element_handler(self, name: str, attributes: list[str]) -> None: ...
    def end_element_handler(self, name: str) -> None: ...  # only exists if __debug__

class ExpatBuilderNS(Namespaces, ExpatBuilder):
    """Document builder that supports namespaces."""

class FragmentBuilderNS(Namespaces, FragmentBuilder):
    """Fragment builder that supports namespaces."""

class ParseEscape(Exception):
    """Exception raised to short-circuit parsing in InternalSubsetExtractor."""

class InternalSubsetExtractor(ExpatBuilder):
    """XML processor which can rip out the internal document type subset."""

    subset: str | list[str] | None = None
    def getSubset(self) -> str:
        """Return the internal subset as a string."""

    def parseFile(self, file: SupportsRead[ReadableBuffer | str]) -> None: ...  # type: ignore[override]
    def parseString(self, string: str | ReadableBuffer) -> None: ...  # type: ignore[override]
    def start_doctype_decl_handler(  # type: ignore[override]
        self, name: str, publicId: str | None, systemId: str | None, has_internal_subset: bool
    ) -> None: ...
    def end_doctype_decl_handler(self) -> NoReturn: ...
    def start_element_handler(self, name: str, attrs: list[str]) -> NoReturn: ...

def parse(file: str | SupportsRead[ReadableBuffer | str], namespaces: bool = True) -> Document:
    """Parse a document, returning the resulting Document node.

    'file' may be either a file name or an open file object.
    """

def parseString(string: str | ReadableBuffer, namespaces: bool = True) -> Document:
    """Parse a document from a string, returning the resulting
    Document node.
    """

def parseFragment(file: str | SupportsRead[ReadableBuffer | str], context: Node, namespaces: bool = True) -> DocumentFragment:
    """Parse a fragment of a document, given the context from which it
    was originally extracted.  context should be the parent of the
    node(s) which are in the fragment.

    'file' may be either a file name or an open file object.
    """

def parseFragmentString(string: str | ReadableBuffer, context: Node, namespaces: bool = True) -> DocumentFragment:
    """Parse a fragment of a document from a string, given the context
    from which it was originally extracted.  context should be the
    parent of the node(s) which are in the fragment.
    """

def makeBuilder(options: Options) -> ExpatBuilderNS | ExpatBuilder:
    """Create a builder based on an Options object."""
