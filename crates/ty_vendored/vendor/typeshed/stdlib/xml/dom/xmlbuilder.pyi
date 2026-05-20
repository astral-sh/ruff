from _typeshed import SupportsRead
from typing import Any, Final, Literal, NoReturn
from xml.dom.minidom import Document, Node, _DOMErrorHandler

__all__ = ["DOMBuilder", "DOMEntityResolver", "DOMInputSource"]

class Options:
    namespaces: int
    namespace_declarations: bool
    validation: bool
    external_parameter_entities: bool
    external_general_entities: bool
    external_dtd_subset: bool
    validate_if_schema: bool
    validate: bool
    datatype_normalization: bool
    create_entity_ref_nodes: bool
    entities: bool
    whitespace_in_element_content: bool
    cdata_sections: bool
    comments: bool
    charset_overrides_xml_encoding: bool
    infoset: bool
    supported_mediatypes_only: bool
    errorHandler: _DOMErrorHandler | None
    filter: DOMBuilderFilter | None

class DOMBuilder:
    entityResolver: DOMEntityResolver | None
    errorHandler: _DOMErrorHandler | None
    filter: DOMBuilderFilter | None
    ACTION_REPLACE: Final = 1
    ACTION_APPEND_AS_CHILDREN: Final = 2
    ACTION_INSERT_AFTER: Final = 3
    ACTION_INSERT_BEFORE: Final = 4
    def __init__(self) -> None: ...
    def setFeature(self, name: str, state: int) -> None: ...
    def supportsFeature(self, name: str) -> bool: ...
    def canSetFeature(self, name: str, state: Literal[1, 0]) -> bool: ...
    # getFeature could return any attribute from an instance of `Options`
    def getFeature(self, name: str) -> Any: ...
    def parseURI(self, uri: str) -> Document: ...
    def parse(self, input: DOMInputSource) -> Document: ...
    def parseWithContext(self, input: DOMInputSource, cnode: Node, action: Literal[1, 2, 3, 4]) -> NoReturn: ...

class DOMEntityResolver:
    __slots__ = ("_opener",)
    def resolveEntity(self, publicId: str | None, systemId: str) -> DOMInputSource: ...

class DOMInputSource:
    __slots__ = ("byteStream", "characterStream", "stringData", "encoding", "publicId", "systemId", "baseURI")
    byteStream: SupportsRead[bytes] | None
    characterStream: SupportsRead[str] | None
    stringData: str | None
    encoding: str | None
    publicId: str | None
    systemId: str | None
    baseURI: str | None

class DOMBuilderFilter:
    FILTER_ACCEPT: Final = 1
    FILTER_REJECT: Final = 2
    FILTER_SKIP: Final = 3
    FILTER_INTERRUPT: Final = 4
    whatToShow: int
    def acceptNode(self, element: Node) -> Literal[1, 2, 3, 4]: ...
    def startContainer(self, element: Node) -> Literal[1, 2, 3, 4]: ...

class DocumentLS:
    async_: bool
    def abort(self) -> NoReturn: ...
    def load(self, uri: str) -> NoReturn: ...
    def loadXML(self, source: str) -> NoReturn: ...
    def saveXML(self, snode: Node | None) -> str: ...

class DOMImplementationLS:
    MODE_SYNCHRONOUS: Final = 1
    MODE_ASYNCHRONOUS: Final = 2
    def createDOMBuilder(self, mode: Literal[1], schemaType: None) -> DOMBuilder: ...
    def createDOMWriter(self) -> NoReturn: ...
    def createDOMInputSource(self) -> DOMInputSource: ...
