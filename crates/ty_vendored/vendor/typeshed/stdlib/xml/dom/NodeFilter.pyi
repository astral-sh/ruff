from typing import Literal
from xml.dom.minidom import Node

class NodeFilter:
    FILTER_ACCEPT: Literal[1]
    FILTER_REJECT: Literal[2]
    FILTER_SKIP: Literal[3]

    SHOW_ALL: int
    SHOW_ELEMENT: int
    SHOW_ATTRIBUTE: int
    SHOW_TEXT: int
    SHOW_CDATA_SECTION: int
    SHOW_ENTITY_REFERENCE: int
    SHOW_ENTITY: int
    SHOW_PROCESSING_INSTRUCTION: int
    SHOW_COMMENT: int
    SHOW_DOCUMENT: int
    SHOW_DOCUMENT_TYPE: int
    SHOW_DOCUMENT_FRAGMENT: int
    SHOW_NOTATION: int
    def acceptNode(self, node: Node) -> int: ...
