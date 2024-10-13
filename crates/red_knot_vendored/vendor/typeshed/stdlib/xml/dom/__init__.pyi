from typing import Any, Final

from .domreg import getDOMImplementation as getDOMImplementation, registerDOMImplementation as registerDOMImplementation

class Node:
    ELEMENT_NODE: int
    ATTRIBUTE_NODE: int
    TEXT_NODE: int
    CDATA_SECTION_NODE: int
    ENTITY_REFERENCE_NODE: int
    ENTITY_NODE: int
    PROCESSING_INSTRUCTION_NODE: int
    COMMENT_NODE: int
    DOCUMENT_NODE: int
    DOCUMENT_TYPE_NODE: int
    DOCUMENT_FRAGMENT_NODE: int
    NOTATION_NODE: int

# ExceptionCode
INDEX_SIZE_ERR: Final[int]
DOMSTRING_SIZE_ERR: Final[int]
HIERARCHY_REQUEST_ERR: Final[int]
WRONG_DOCUMENT_ERR: Final[int]
INVALID_CHARACTER_ERR: Final[int]
NO_DATA_ALLOWED_ERR: Final[int]
NO_MODIFICATION_ALLOWED_ERR: Final[int]
NOT_FOUND_ERR: Final[int]
NOT_SUPPORTED_ERR: Final[int]
INUSE_ATTRIBUTE_ERR: Final[int]
INVALID_STATE_ERR: Final[int]
SYNTAX_ERR: Final[int]
INVALID_MODIFICATION_ERR: Final[int]
NAMESPACE_ERR: Final[int]
INVALID_ACCESS_ERR: Final[int]
VALIDATION_ERR: Final[int]

class DOMException(Exception):
    code: int
    def __init__(self, *args: Any, **kw: Any) -> None: ...
    def _get_code(self) -> int: ...

class IndexSizeErr(DOMException): ...
class DomstringSizeErr(DOMException): ...
class HierarchyRequestErr(DOMException): ...
class WrongDocumentErr(DOMException): ...
class InvalidCharacterErr(DOMException): ...
class NoDataAllowedErr(DOMException): ...
class NoModificationAllowedErr(DOMException): ...
class NotFoundErr(DOMException): ...
class NotSupportedErr(DOMException): ...
class InuseAttributeErr(DOMException): ...
class InvalidStateErr(DOMException): ...
class SyntaxErr(DOMException): ...
class InvalidModificationErr(DOMException): ...
class NamespaceErr(DOMException): ...
class InvalidAccessErr(DOMException): ...
class ValidationErr(DOMException): ...

class UserDataHandler:
    NODE_CLONED: int
    NODE_IMPORTED: int
    NODE_DELETED: int
    NODE_RENAMED: int

XML_NAMESPACE: Final[str]
XMLNS_NAMESPACE: Final[str]
XHTML_NAMESPACE: Final[str]
EMPTY_NAMESPACE: Final[None]
EMPTY_PREFIX: Final[None]
