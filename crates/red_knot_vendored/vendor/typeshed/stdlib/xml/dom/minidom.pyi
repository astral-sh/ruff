import sys
import xml.dom
from _collections_abc import dict_keys, dict_values
from _typeshed import Incomplete, ReadableBuffer, SupportsRead, SupportsWrite
from collections.abc import Iterable, Sequence
from types import TracebackType
from typing import Any, ClassVar, Generic, Literal, NoReturn, Protocol, TypeVar, overload
from typing_extensions import Self, TypeAlias
from xml.dom.minicompat import EmptyNodeList, NodeList
from xml.dom.xmlbuilder import DocumentLS, DOMImplementationLS
from xml.sax.xmlreader import XMLReader

_NSName: TypeAlias = tuple[str | None, str]

# Entity can also have children, but it's not implemented the same way as the
# others, so is deliberately omitted here.
_NodesWithChildren: TypeAlias = DocumentFragment | Attr | Element | Document
_NodesThatAreChildren: TypeAlias = CDATASection | Comment | DocumentType | Element | Notation | ProcessingInstruction | Text

_AttrChildren: TypeAlias = Text  # Also EntityReference, but we don't implement it
_ElementChildren: TypeAlias = Element | ProcessingInstruction | Comment | Text | CDATASection
_EntityChildren: TypeAlias = Text  # I think; documentation is a little unclear
_DocumentFragmentChildren: TypeAlias = Element | Text | CDATASection | ProcessingInstruction | Comment | Notation
_DocumentChildren: TypeAlias = Comment | DocumentType | Element | ProcessingInstruction

_N = TypeVar("_N", bound=Node)
_ChildNodeVar = TypeVar("_ChildNodeVar", bound=_NodesThatAreChildren)
_ChildNodePlusFragmentVar = TypeVar("_ChildNodePlusFragmentVar", bound=_NodesThatAreChildren | DocumentFragment)
_DocumentChildrenVar = TypeVar("_DocumentChildrenVar", bound=_DocumentChildren)
_ImportableNodeVar = TypeVar(
    "_ImportableNodeVar",
    bound=DocumentFragment
    | Attr
    | Element
    | ProcessingInstruction
    | CharacterData
    | Text
    | Comment
    | CDATASection
    | Entity
    | Notation,
)

class _DOMErrorHandler(Protocol):
    def handleError(self, error: Exception) -> bool: ...

class _UserDataHandler(Protocol):
    def handle(self, operation: int, key: str, data: Any, src: Node, dst: Node) -> None: ...

def parse(
    file: str | SupportsRead[ReadableBuffer | str], parser: XMLReader | None = None, bufsize: int | None = None
) -> Document: ...
def parseString(string: str | ReadableBuffer, parser: XMLReader | None = None) -> Document: ...
@overload
def getDOMImplementation(features: None = None) -> DOMImplementation: ...
@overload
def getDOMImplementation(features: str | Iterable[tuple[str, str | None]]) -> DOMImplementation | None: ...

class Node(xml.dom.Node):
    parentNode: _NodesWithChildren | Entity | None
    ownerDocument: Document | None
    nextSibling: _NodesThatAreChildren | None
    previousSibling: _NodesThatAreChildren | None
    namespaceURI: str | None  # non-null only for Element and Attr
    prefix: str | None  # non-null only for NS Element and Attr

    # These aren't defined on Node, but they exist on all Node subclasses
    # and various methods of Node require them to exist.
    childNodes: (
        NodeList[_DocumentFragmentChildren]
        | NodeList[_AttrChildren]
        | NodeList[_ElementChildren]
        | NodeList[_DocumentChildren]
        | NodeList[_EntityChildren]
        | EmptyNodeList
    )
    nodeType: ClassVar[Literal[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]]
    nodeName: str | None  # only possibly None on DocumentType

    # Not defined on Node, but exist on all Node subclasses.
    nodeValue: str | None  # non-null for Attr, ProcessingInstruction, Text, Comment, and CDATASection
    attributes: NamedNodeMap | None  # non-null only for Element

    @property
    def firstChild(self) -> _NodesThatAreChildren | None: ...
    @property
    def lastChild(self) -> _NodesThatAreChildren | None: ...
    @property
    def localName(self) -> str | None: ...  # non-null only for Element and Attr
    def __bool__(self) -> Literal[True]: ...
    if sys.version_info >= (3, 9):
        @overload
        def toxml(self, encoding: str, standalone: bool | None = None) -> bytes: ...
        @overload
        def toxml(self, encoding: None = None, standalone: bool | None = None) -> str: ...
        @overload
        def toprettyxml(
            self,
            indent: str = "\t",
            newl: str = "\n",
            # Handle any case where encoding is not provided or where it is passed with None
            encoding: None = None,
            standalone: bool | None = None,
        ) -> str: ...
        @overload
        def toprettyxml(
            self,
            indent: str,
            newl: str,
            # Handle cases where encoding is passed as str *positionally*
            encoding: str,
            standalone: bool | None = None,
        ) -> bytes: ...
        @overload
        def toprettyxml(
            self,
            indent: str = "\t",
            newl: str = "\n",
            # Handle all cases where encoding is passed as a keyword argument; because standalone
            # comes after, it will also have to be a keyword arg if encoding is
            *,
            encoding: str,
            standalone: bool | None = None,
        ) -> bytes: ...
    else:
        @overload
        def toxml(self, encoding: str) -> bytes: ...
        @overload
        def toxml(self, encoding: None = None) -> str: ...
        @overload
        def toprettyxml(
            self,
            indent: str = "\t",
            newl: str = "\n",
            # Handle any case where encoding is not provided or where it is passed with None
            encoding: None = None,
        ) -> str: ...
        @overload
        def toprettyxml(
            self,
            indent: str,
            newl: str,
            # Handle cases where encoding is passed as str *positionally*
            encoding: str,
        ) -> bytes: ...
        @overload
        def toprettyxml(
            self,
            indent: str = "\t",
            newl: str = "\n",
            # Handle all cases where encoding is passed as a keyword argument
            *,
            encoding: str,
        ) -> bytes: ...

    def hasChildNodes(self) -> bool: ...
    def insertBefore(  # type: ignore[misc]
        self: _NodesWithChildren,  # pyright: ignore[reportGeneralTypeIssues]
        newChild: _ChildNodePlusFragmentVar,
        refChild: _NodesThatAreChildren | None,
    ) -> _ChildNodePlusFragmentVar: ...
    def appendChild(  # type: ignore[misc]
        self: _NodesWithChildren, node: _ChildNodePlusFragmentVar  # pyright: ignore[reportGeneralTypeIssues]
    ) -> _ChildNodePlusFragmentVar: ...
    @overload
    def replaceChild(  # type: ignore[misc]
        self: _NodesWithChildren, newChild: DocumentFragment, oldChild: _ChildNodeVar
    ) -> _ChildNodeVar | DocumentFragment: ...
    @overload
    def replaceChild(  # type: ignore[misc]
        self: _NodesWithChildren, newChild: _NodesThatAreChildren, oldChild: _ChildNodeVar
    ) -> _ChildNodeVar | None: ...
    def removeChild(self: _NodesWithChildren, oldChild: _ChildNodeVar) -> _ChildNodeVar: ...  # type: ignore[misc]  # pyright: ignore[reportGeneralTypeIssues]
    def normalize(self: _NodesWithChildren) -> None: ...  # type: ignore[misc]  # pyright: ignore[reportGeneralTypeIssues]
    def cloneNode(self, deep: bool) -> Self | None: ...
    def isSupported(self, feature: str, version: str | None) -> bool: ...
    def isSameNode(self, other: Node) -> bool: ...
    def getInterface(self, feature: str) -> Self | None: ...
    def getUserData(self, key: str) -> Any | None: ...
    def setUserData(self, key: str, data: Any, handler: _UserDataHandler) -> Any: ...
    def unlink(self) -> None: ...
    def __enter__(self) -> Self: ...
    def __exit__(self, et: type[BaseException] | None, ev: BaseException | None, tb: TracebackType | None) -> None: ...

_DFChildrenVar = TypeVar("_DFChildrenVar", bound=_DocumentFragmentChildren)
_DFChildrenPlusFragment = TypeVar("_DFChildrenPlusFragment", bound=_DocumentFragmentChildren | DocumentFragment)

class DocumentFragment(Node):
    nodeType: ClassVar[Literal[11]]
    nodeName: Literal["#document-fragment"]
    nodeValue: None
    attributes: None

    parentNode: None
    nextSibling: None
    previousSibling: None
    childNodes: NodeList[_DocumentFragmentChildren]
    @property
    def firstChild(self) -> _DocumentFragmentChildren | None: ...
    @property
    def lastChild(self) -> _DocumentFragmentChildren | None: ...

    namespaceURI: None
    prefix: None
    @property
    def localName(self) -> None: ...
    def __init__(self) -> None: ...
    def insertBefore(  # type: ignore[override]
        self, newChild: _DFChildrenPlusFragment, refChild: _DocumentFragmentChildren | None
    ) -> _DFChildrenPlusFragment: ...
    def appendChild(self, node: _DFChildrenPlusFragment) -> _DFChildrenPlusFragment: ...  # type: ignore[override]
    @overload  # type: ignore[override]
    def replaceChild(self, newChild: DocumentFragment, oldChild: _DFChildrenVar) -> _DFChildrenVar | DocumentFragment: ...
    @overload
    def replaceChild(self, newChild: _DocumentFragmentChildren, oldChild: _DFChildrenVar) -> _DFChildrenVar | None: ...  # type: ignore[override]
    def removeChild(self, oldChild: _DFChildrenVar) -> _DFChildrenVar: ...  # type: ignore[override]

_AttrChildrenVar = TypeVar("_AttrChildrenVar", bound=_AttrChildren)
_AttrChildrenPlusFragment = TypeVar("_AttrChildrenPlusFragment", bound=_AttrChildren | DocumentFragment)

class Attr(Node):
    nodeType: ClassVar[Literal[2]]
    nodeName: str  # same as Attr.name
    nodeValue: str  # same as Attr.value
    attributes: None

    parentNode: None
    nextSibling: None
    previousSibling: None
    childNodes: NodeList[_AttrChildren]
    @property
    def firstChild(self) -> _AttrChildren | None: ...
    @property
    def lastChild(self) -> _AttrChildren | None: ...

    namespaceURI: str | None
    prefix: str | None
    @property
    def localName(self) -> str: ...

    name: str
    value: str
    specified: bool
    ownerElement: Element | None

    def __init__(
        self, qName: str, namespaceURI: str | None = None, localName: str | None = None, prefix: str | None = None
    ) -> None: ...
    def unlink(self) -> None: ...
    @property
    def isId(self) -> bool: ...
    @property
    def schemaType(self) -> TypeInfo: ...
    def insertBefore(self, newChild: _AttrChildrenPlusFragment, refChild: _AttrChildren | None) -> _AttrChildrenPlusFragment: ...  # type: ignore[override]
    def appendChild(self, node: _AttrChildrenPlusFragment) -> _AttrChildrenPlusFragment: ...  # type: ignore[override]
    @overload  # type: ignore[override]
    def replaceChild(self, newChild: DocumentFragment, oldChild: _AttrChildrenVar) -> _AttrChildrenVar | DocumentFragment: ...
    @overload
    def replaceChild(self, newChild: _AttrChildren, oldChild: _AttrChildrenVar) -> _AttrChildrenVar | None: ...  # type: ignore[override]
    def removeChild(self, oldChild: _AttrChildrenVar) -> _AttrChildrenVar: ...  # type: ignore[override]

# In the DOM, this interface isn't specific to Attr, but our implementation is
# because that's the only place we use it.
class NamedNodeMap:
    def __init__(self, attrs: dict[str, Attr], attrsNS: dict[_NSName, Attr], ownerElement: Element) -> None: ...
    @property
    def length(self) -> int: ...
    def item(self, index: int) -> Node | None: ...
    def items(self) -> list[tuple[str, str]]: ...
    def itemsNS(self) -> list[tuple[_NSName, str]]: ...
    def __contains__(self, key: str | _NSName) -> bool: ...
    def keys(self) -> dict_keys[str, Attr]: ...
    def keysNS(self) -> dict_keys[_NSName, Attr]: ...
    def values(self) -> dict_values[str, Attr]: ...
    def get(self, name: str, value: Attr | None = None) -> Attr | None: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]
    def __len__(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...
    def __ge__(self, other: NamedNodeMap) -> bool: ...
    def __gt__(self, other: NamedNodeMap) -> bool: ...
    def __le__(self, other: NamedNodeMap) -> bool: ...
    def __lt__(self, other: NamedNodeMap) -> bool: ...
    def __getitem__(self, attname_or_tuple: _NSName | str) -> Attr: ...
    def __setitem__(self, attname: str, value: Attr | str) -> None: ...
    def getNamedItem(self, name: str) -> Attr | None: ...
    def getNamedItemNS(self, namespaceURI: str | None, localName: str) -> Attr | None: ...
    def removeNamedItem(self, name: str) -> Attr: ...
    def removeNamedItemNS(self, namespaceURI: str | None, localName: str) -> Attr: ...
    def setNamedItem(self, node: Attr) -> Attr | None: ...
    def setNamedItemNS(self, node: Attr) -> Attr | None: ...
    def __delitem__(self, attname_or_tuple: _NSName | str) -> None: ...

AttributeList = NamedNodeMap

class TypeInfo:
    namespace: str | None
    name: str | None
    def __init__(self, namespace: Incomplete | None, name: str | None) -> None: ...

_ElementChildrenVar = TypeVar("_ElementChildrenVar", bound=_ElementChildren)
_ElementChildrenPlusFragment = TypeVar("_ElementChildrenPlusFragment", bound=_ElementChildren | DocumentFragment)

class Element(Node):
    nodeType: ClassVar[Literal[1]]
    nodeName: str  # same as Element.tagName
    nodeValue: None
    @property
    def attributes(self) -> NamedNodeMap: ...  # type: ignore[override]

    parentNode: Document | Element | DocumentFragment | None
    nextSibling: _DocumentChildren | _ElementChildren | _DocumentFragmentChildren | None
    previousSibling: _DocumentChildren | _ElementChildren | _DocumentFragmentChildren | None
    childNodes: NodeList[_ElementChildren]
    @property
    def firstChild(self) -> _ElementChildren | None: ...
    @property
    def lastChild(self) -> _ElementChildren | None: ...

    namespaceURI: str | None
    prefix: str | None
    @property
    def localName(self) -> str: ...

    schemaType: TypeInfo
    tagName: str

    def __init__(
        self, tagName: str, namespaceURI: str | None = None, prefix: str | None = None, localName: str | None = None
    ) -> None: ...
    def unlink(self) -> None: ...
    def getAttribute(self, attname: str) -> str: ...
    def getAttributeNS(self, namespaceURI: str | None, localName: str) -> str: ...
    def setAttribute(self, attname: str, value: str) -> None: ...
    def setAttributeNS(self, namespaceURI: str | None, qualifiedName: str, value: str) -> None: ...
    def getAttributeNode(self, attrname: str) -> Attr | None: ...
    def getAttributeNodeNS(self, namespaceURI: str | None, localName: str) -> Attr | None: ...
    def setAttributeNode(self, attr: Attr) -> Attr | None: ...
    setAttributeNodeNS = setAttributeNode
    def removeAttribute(self, name: str) -> None: ...
    def removeAttributeNS(self, namespaceURI: str | None, localName: str) -> None: ...
    def removeAttributeNode(self, node: Attr) -> Attr: ...
    removeAttributeNodeNS = removeAttributeNode
    def hasAttribute(self, name: str) -> bool: ...
    def hasAttributeNS(self, namespaceURI: str | None, localName: str) -> bool: ...
    def getElementsByTagName(self, name: str) -> NodeList[Element]: ...
    def getElementsByTagNameNS(self, namespaceURI: str | None, localName: str) -> NodeList[Element]: ...
    def writexml(self, writer: SupportsWrite[str], indent: str = "", addindent: str = "", newl: str = "") -> None: ...
    def hasAttributes(self) -> bool: ...
    def setIdAttribute(self, name: str) -> None: ...
    def setIdAttributeNS(self, namespaceURI: str | None, localName: str) -> None: ...
    def setIdAttributeNode(self, idAttr: Attr) -> None: ...
    def insertBefore(  # type: ignore[override]
        self, newChild: _ElementChildrenPlusFragment, refChild: _ElementChildren | None
    ) -> _ElementChildrenPlusFragment: ...
    def appendChild(self, node: _ElementChildrenPlusFragment) -> _ElementChildrenPlusFragment: ...  # type: ignore[override]
    @overload  # type: ignore[override]
    def replaceChild(
        self, newChild: DocumentFragment, oldChild: _ElementChildrenVar
    ) -> _ElementChildrenVar | DocumentFragment: ...
    @overload
    def replaceChild(self, newChild: _ElementChildren, oldChild: _ElementChildrenVar) -> _ElementChildrenVar | None: ...  # type: ignore[override]
    def removeChild(self, oldChild: _ElementChildrenVar) -> _ElementChildrenVar: ...  # type: ignore[override]

class Childless:
    attributes: None
    childNodes: EmptyNodeList
    @property
    def firstChild(self) -> None: ...
    @property
    def lastChild(self) -> None: ...
    def appendChild(self, node: _NodesThatAreChildren | DocumentFragment) -> NoReturn: ...
    def hasChildNodes(self) -> Literal[False]: ...
    def insertBefore(
        self, newChild: _NodesThatAreChildren | DocumentFragment, refChild: _NodesThatAreChildren | None
    ) -> NoReturn: ...
    def removeChild(self, oldChild: _NodesThatAreChildren) -> NoReturn: ...
    def normalize(self) -> None: ...
    def replaceChild(self, newChild: _NodesThatAreChildren | DocumentFragment, oldChild: _NodesThatAreChildren) -> NoReturn: ...

class ProcessingInstruction(Childless, Node):
    nodeType: ClassVar[Literal[7]]
    nodeName: str  # same as ProcessingInstruction.target
    nodeValue: str  # same as ProcessingInstruction.data
    attributes: None

    parentNode: Document | Element | DocumentFragment | None
    nextSibling: _DocumentChildren | _ElementChildren | _DocumentFragmentChildren | None
    previousSibling: _DocumentChildren | _ElementChildren | _DocumentFragmentChildren | None
    childNodes: EmptyNodeList
    @property
    def firstChild(self) -> None: ...
    @property
    def lastChild(self) -> None: ...

    namespaceURI: None
    prefix: None
    @property
    def localName(self) -> None: ...

    target: str
    data: str

    def __init__(self, target: str, data: str) -> None: ...
    def writexml(self, writer: SupportsWrite[str], indent: str = "", addindent: str = "", newl: str = "") -> None: ...

class CharacterData(Childless, Node):
    nodeValue: str
    attributes: None

    childNodes: EmptyNodeList
    nextSibling: _NodesThatAreChildren | None
    previousSibling: _NodesThatAreChildren | None

    @property
    def localName(self) -> None: ...

    ownerDocument: Document | None
    data: str

    def __init__(self) -> None: ...
    @property
    def length(self) -> int: ...
    def __len__(self) -> int: ...
    def substringData(self, offset: int, count: int) -> str: ...
    def appendData(self, arg: str) -> None: ...
    def insertData(self, offset: int, arg: str) -> None: ...
    def deleteData(self, offset: int, count: int) -> None: ...
    def replaceData(self, offset: int, count: int, arg: str) -> None: ...

class Text(CharacterData):
    nodeType: ClassVar[Literal[3]]
    nodeName: Literal["#text"]
    nodeValue: str  # same as CharacterData.data, the content of the text node
    attributes: None

    parentNode: Attr | Element | DocumentFragment | None
    nextSibling: _DocumentFragmentChildren | _ElementChildren | _AttrChildren | None
    previousSibling: _DocumentFragmentChildren | _ElementChildren | _AttrChildren | None
    childNodes: EmptyNodeList
    @property
    def firstChild(self) -> None: ...
    @property
    def lastChild(self) -> None: ...

    namespaceURI: None
    prefix: None
    @property
    def localName(self) -> None: ...

    data: str
    def splitText(self, offset: int) -> Self: ...
    def writexml(self, writer: SupportsWrite[str], indent: str = "", addindent: str = "", newl: str = "") -> None: ...
    def replaceWholeText(self, content: str) -> Self | None: ...
    @property
    def isWhitespaceInElementContent(self) -> bool: ...
    @property
    def wholeText(self) -> str: ...

class Comment(CharacterData):
    nodeType: ClassVar[Literal[8]]
    nodeName: Literal["#comment"]
    nodeValue: str  # same as CharacterData.data, the content of the comment
    attributes: None

    parentNode: Document | Element | DocumentFragment | None
    nextSibling: _DocumentChildren | _ElementChildren | _DocumentFragmentChildren | None
    previousSibling: _DocumentChildren | _ElementChildren | _DocumentFragmentChildren | None
    childNodes: EmptyNodeList
    @property
    def firstChild(self) -> None: ...
    @property
    def lastChild(self) -> None: ...

    namespaceURI: None
    prefix: None
    @property
    def localName(self) -> None: ...
    def __init__(self, data: str) -> None: ...
    def writexml(self, writer: SupportsWrite[str], indent: str = "", addindent: str = "", newl: str = "") -> None: ...

class CDATASection(Text):
    nodeType: ClassVar[Literal[4]]  # type: ignore[assignment]
    nodeName: Literal["#cdata-section"]  # type: ignore[assignment]
    nodeValue: str  # same as CharacterData.data, the content of the CDATA Section
    attributes: None

    parentNode: Element | DocumentFragment | None
    nextSibling: _DocumentFragmentChildren | _ElementChildren | None
    previousSibling: _DocumentFragmentChildren | _ElementChildren | None

    def writexml(self, writer: SupportsWrite[str], indent: str = "", addindent: str = "", newl: str = "") -> None: ...

class ReadOnlySequentialNamedNodeMap(Generic[_N]):
    def __init__(self, seq: Sequence[_N] = ()) -> None: ...
    def __len__(self) -> int: ...
    def getNamedItem(self, name: str) -> _N | None: ...
    def getNamedItemNS(self, namespaceURI: str | None, localName: str) -> _N | None: ...
    def __getitem__(self, name_or_tuple: str | _NSName) -> _N | None: ...
    def item(self, index: int) -> _N | None: ...
    def removeNamedItem(self, name: str) -> NoReturn: ...
    def removeNamedItemNS(self, namespaceURI: str | None, localName: str) -> NoReturn: ...
    def setNamedItem(self, node: Node) -> NoReturn: ...
    def setNamedItemNS(self, node: Node) -> NoReturn: ...
    @property
    def length(self) -> int: ...

class Identified:
    publicId: str | None
    systemId: str | None

class DocumentType(Identified, Childless, Node):
    nodeType: ClassVar[Literal[10]]
    nodeName: str | None  # same as DocumentType.name
    nodeValue: None
    attributes: None

    parentNode: Document | None
    nextSibling: _DocumentChildren | None
    previousSibling: _DocumentChildren | None
    childNodes: EmptyNodeList
    @property
    def firstChild(self) -> None: ...
    @property
    def lastChild(self) -> None: ...

    namespaceURI: None
    prefix: None
    @property
    def localName(self) -> None: ...

    name: str | None
    internalSubset: str | None
    entities: ReadOnlySequentialNamedNodeMap[Entity]
    notations: ReadOnlySequentialNamedNodeMap[Notation]

    def __init__(self, qualifiedName: str | None) -> None: ...
    def cloneNode(self, deep: bool) -> DocumentType | None: ...
    def writexml(self, writer: SupportsWrite[str], indent: str = "", addindent: str = "", newl: str = "") -> None: ...

class Entity(Identified, Node):
    nodeType: ClassVar[Literal[6]]
    nodeName: str  # entity name
    nodeValue: None
    attributes: None

    parentNode: None
    nextSibling: None
    previousSibling: None
    childNodes: NodeList[_EntityChildren]
    @property
    def firstChild(self) -> _EntityChildren | None: ...
    @property
    def lastChild(self) -> _EntityChildren | None: ...

    namespaceURI: None
    prefix: None
    @property
    def localName(self) -> None: ...

    actualEncoding: str | None
    encoding: str | None
    version: str | None
    notationName: str | None

    def __init__(self, name: str, publicId: str | None, systemId: str | None, notation: str | None) -> None: ...
    def appendChild(self, newChild: _EntityChildren) -> NoReturn: ...  # type: ignore[override]
    def insertBefore(self, newChild: _EntityChildren, refChild: _EntityChildren | None) -> NoReturn: ...  # type: ignore[override]
    def removeChild(self, oldChild: _EntityChildren) -> NoReturn: ...  # type: ignore[override]
    def replaceChild(self, newChild: _EntityChildren, oldChild: _EntityChildren) -> NoReturn: ...  # type: ignore[override]

class Notation(Identified, Childless, Node):
    nodeType: ClassVar[Literal[12]]
    nodeName: str  # notation name
    nodeValue: None
    attributes: None

    parentNode: DocumentFragment | None
    nextSibling: _DocumentFragmentChildren | None
    previousSibling: _DocumentFragmentChildren | None
    childNodes: EmptyNodeList
    @property
    def firstChild(self) -> None: ...
    @property
    def lastChild(self) -> None: ...

    namespaceURI: None
    prefix: None
    @property
    def localName(self) -> None: ...
    def __init__(self, name: str, publicId: str | None, systemId: str | None) -> None: ...

class DOMImplementation(DOMImplementationLS):
    def hasFeature(self, feature: str, version: str | None) -> bool: ...
    def createDocument(self, namespaceURI: str | None, qualifiedName: str | None, doctype: DocumentType | None) -> Document: ...
    def createDocumentType(self, qualifiedName: str | None, publicId: str | None, systemId: str | None) -> DocumentType: ...
    def getInterface(self, feature: str) -> Self | None: ...

class ElementInfo:
    tagName: str
    def __init__(self, name: str) -> None: ...
    def getAttributeType(self, aname: str) -> TypeInfo: ...
    def getAttributeTypeNS(self, namespaceURI: str | None, localName: str) -> TypeInfo: ...
    def isElementContent(self) -> bool: ...
    def isEmpty(self) -> bool: ...
    def isId(self, aname: str) -> bool: ...
    def isIdNS(self, namespaceURI: str | None, localName: str) -> bool: ...

_DocumentChildrenPlusFragment = TypeVar("_DocumentChildrenPlusFragment", bound=_DocumentChildren | DocumentFragment)

class Document(Node, DocumentLS):
    nodeType: ClassVar[Literal[9]]
    nodeName: Literal["#document"]
    nodeValue: None
    attributes: None

    parentNode: None
    previousSibling: None
    nextSibling: None
    childNodes: NodeList[_DocumentChildren]
    @property
    def firstChild(self) -> _DocumentChildren | None: ...
    @property
    def lastChild(self) -> _DocumentChildren | None: ...

    namespaceURI: None
    prefix: None
    @property
    def localName(self) -> None: ...

    implementation: DOMImplementation
    actualEncoding: str | None
    encoding: str | None
    standalone: bool | None
    version: str | None
    strictErrorChecking: bool
    errorHandler: _DOMErrorHandler | None
    documentURI: str | None
    doctype: DocumentType | None
    documentElement: Element | None

    def __init__(self) -> None: ...
    def appendChild(self, node: _DocumentChildrenVar) -> _DocumentChildrenVar: ...  # type: ignore[override]
    def removeChild(self, oldChild: _DocumentChildrenVar) -> _DocumentChildrenVar: ...  # type: ignore[override]
    def unlink(self) -> None: ...
    def cloneNode(self, deep: bool) -> Document | None: ...
    def createDocumentFragment(self) -> DocumentFragment: ...
    def createElement(self, tagName: str) -> Element: ...
    def createTextNode(self, data: str) -> Text: ...
    def createCDATASection(self, data: str) -> CDATASection: ...
    def createComment(self, data: str) -> Comment: ...
    def createProcessingInstruction(self, target: str, data: str) -> ProcessingInstruction: ...
    def createAttribute(self, qName: str) -> Attr: ...
    def createElementNS(self, namespaceURI: str | None, qualifiedName: str) -> Element: ...
    def createAttributeNS(self, namespaceURI: str | None, qualifiedName: str) -> Attr: ...
    def getElementById(self, id: str) -> Element | None: ...
    def getElementsByTagName(self, name: str) -> NodeList[Element]: ...
    def getElementsByTagNameNS(self, namespaceURI: str | None, localName: str) -> NodeList[Element]: ...
    def isSupported(self, feature: str, version: str | None) -> bool: ...
    def importNode(self, node: _ImportableNodeVar, deep: bool) -> _ImportableNodeVar: ...
    if sys.version_info >= (3, 9):
        def writexml(
            self,
            writer: SupportsWrite[str],
            indent: str = "",
            addindent: str = "",
            newl: str = "",
            encoding: str | None = None,
            standalone: bool | None = None,
        ) -> None: ...
    else:
        def writexml(
            self,
            writer: SupportsWrite[str],
            indent: str = "",
            addindent: str = "",
            newl: str = "",
            encoding: Incomplete | None = None,
        ) -> None: ...

    @overload
    def renameNode(self, n: Element, namespaceURI: str, name: str) -> Element: ...
    @overload
    def renameNode(self, n: Attr, namespaceURI: str, name: str) -> Attr: ...
    @overload
    def renameNode(self, n: Element | Attr, namespaceURI: str, name: str) -> Element | Attr: ...
    def insertBefore(
        self, newChild: _DocumentChildrenPlusFragment, refChild: _DocumentChildren | None  # type: ignore[override]
    ) -> _DocumentChildrenPlusFragment: ...
    @overload  # type: ignore[override]
    def replaceChild(
        self, newChild: DocumentFragment, oldChild: _DocumentChildrenVar
    ) -> _DocumentChildrenVar | DocumentFragment: ...
    @overload
    def replaceChild(self, newChild: _DocumentChildren, oldChild: _DocumentChildrenVar) -> _DocumentChildrenVar | None: ...
