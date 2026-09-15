from typing import Any, Final, Literal

from .domreg import getDOMImplementation as getDOMImplementation, registerDOMImplementation as registerDOMImplementation

class Node:
    __slots__ = ()
    ELEMENT_NODE: Final = 1
    ATTRIBUTE_NODE: Final = 2
    TEXT_NODE: Final = 3
    CDATA_SECTION_NODE: Final = 4
    ENTITY_REFERENCE_NODE: Final = 5
    ENTITY_NODE: Final = 6
    PROCESSING_INSTRUCTION_NODE: Final = 7
    COMMENT_NODE: Final = 8
    DOCUMENT_NODE: Final = 9
    DOCUMENT_TYPE_NODE: Final = 10
    DOCUMENT_FRAGMENT_NODE: Final = 11
    NOTATION_NODE: Final = 12

# ExceptionCode
INDEX_SIZE_ERR: Final = 1
DOMSTRING_SIZE_ERR: Final = 2
HIERARCHY_REQUEST_ERR: Final = 3
WRONG_DOCUMENT_ERR: Final = 4
INVALID_CHARACTER_ERR: Final = 5
NO_DATA_ALLOWED_ERR: Final = 6
NO_MODIFICATION_ALLOWED_ERR: Final = 7
NOT_FOUND_ERR: Final = 8
NOT_SUPPORTED_ERR: Final = 9
INUSE_ATTRIBUTE_ERR: Final = 10
INVALID_STATE_ERR: Final = 11
SYNTAX_ERR: Final = 12
INVALID_MODIFICATION_ERR: Final = 13
NAMESPACE_ERR: Final = 14
INVALID_ACCESS_ERR: Final = 15
VALIDATION_ERR: Final = 16

class DOMException(Exception):
    code: int
    def __init__(self, *args: Any, **kw: Any) -> None: ...
    def _get_code(self) -> int: ...

class IndexSizeErr(DOMException):
    code: Literal[1]

class DomstringSizeErr(DOMException):
    code: Literal[2]

class HierarchyRequestErr(DOMException):
    code: Literal[3]

class WrongDocumentErr(DOMException):
    code: Literal[4]

class InvalidCharacterErr(DOMException):
    code: Literal[5]

class NoDataAllowedErr(DOMException):
    code: Literal[6]

class NoModificationAllowedErr(DOMException):
    code: Literal[7]

class NotFoundErr(DOMException):
    code: Literal[8]

class NotSupportedErr(DOMException):
    code: Literal[9]

class InuseAttributeErr(DOMException):
    code: Literal[10]

class InvalidStateErr(DOMException):
    code: Literal[11]

class SyntaxErr(DOMException):
    code: Literal[12]

class InvalidModificationErr(DOMException):
    code: Literal[13]

class NamespaceErr(DOMException):
    code: Literal[14]

class InvalidAccessErr(DOMException):
    code: Literal[15]

class ValidationErr(DOMException):
    code: Literal[16]

class UserDataHandler:
    NODE_CLONED: Final = 1
    NODE_IMPORTED: Final = 2
    NODE_DELETED: Final = 3
    NODE_RENAMED: Final = 4

XML_NAMESPACE: Final = "http://www.w3.org/XML/1998/namespace"
XMLNS_NAMESPACE: Final = "http://www.w3.org/2000/xmlns/"
XHTML_NAMESPACE: Final = "http://www.w3.org/1999/xhtml"
EMPTY_NAMESPACE: Final[None]
EMPTY_PREFIX: Final[None]
