from typing import Final
from xml.dom.minidom import Node

class NodeFilter:
    FILTER_ACCEPT: Final = 1
    FILTER_REJECT: Final = 2
    FILTER_SKIP: Final = 3

    SHOW_ALL: Final = 0xFFFFFFFF
    SHOW_ELEMENT: Final = 0x00000001
    SHOW_ATTRIBUTE: Final = 0x00000002
    SHOW_TEXT: Final = 0x00000004
    SHOW_CDATA_SECTION: Final = 0x00000008
    SHOW_ENTITY_REFERENCE: Final = 0x00000010
    SHOW_ENTITY: Final = 0x00000020
    SHOW_PROCESSING_INSTRUCTION: Final = 0x00000040
    SHOW_COMMENT: Final = 0x00000080
    SHOW_DOCUMENT: Final = 0x00000100
    SHOW_DOCUMENT_TYPE: Final = 0x00000200
    SHOW_DOCUMENT_FRAGMENT: Final = 0x00000400
    SHOW_NOTATION: Final = 0x00000800
    def acceptNode(self, node: Node) -> int: ...
