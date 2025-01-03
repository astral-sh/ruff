import sys
from _typeshed import ReadableBuffer
from typing import ClassVar, final
from typing_extensions import Self

BLAKE2B_MAX_DIGEST_SIZE: int = 64
BLAKE2B_MAX_KEY_SIZE: int = 64
BLAKE2B_PERSON_SIZE: int = 16
BLAKE2B_SALT_SIZE: int = 16
BLAKE2S_MAX_DIGEST_SIZE: int = 32
BLAKE2S_MAX_KEY_SIZE: int = 32
BLAKE2S_PERSON_SIZE: int = 8
BLAKE2S_SALT_SIZE: int = 8

@final
class blake2b:
    MAX_DIGEST_SIZE: ClassVar[int] = 64
    MAX_KEY_SIZE: ClassVar[int] = 64
    PERSON_SIZE: ClassVar[int] = 16
    SALT_SIZE: ClassVar[int] = 16
    block_size: int
    digest_size: int
    name: str
    if sys.version_info >= (3, 9):
        def __new__(
            cls,
            data: ReadableBuffer = b"",
            /,
            *,
            digest_size: int = 64,
            key: ReadableBuffer = b"",
            salt: ReadableBuffer = b"",
            person: ReadableBuffer = b"",
            fanout: int = 1,
            depth: int = 1,
            leaf_size: int = 0,
            node_offset: int = 0,
            node_depth: int = 0,
            inner_size: int = 0,
            last_node: bool = False,
            usedforsecurity: bool = True,
        ) -> Self: ...
    else:
        def __new__(
            cls,
            data: ReadableBuffer = b"",
            /,
            *,
            digest_size: int = 64,
            key: ReadableBuffer = b"",
            salt: ReadableBuffer = b"",
            person: ReadableBuffer = b"",
            fanout: int = 1,
            depth: int = 1,
            leaf_size: int = 0,
            node_offset: int = 0,
            node_depth: int = 0,
            inner_size: int = 0,
            last_node: bool = False,
        ) -> Self: ...

    def copy(self) -> Self: ...
    def digest(self) -> bytes: ...
    def hexdigest(self) -> str: ...
    def update(self, data: ReadableBuffer, /) -> None: ...

@final
class blake2s:
    MAX_DIGEST_SIZE: ClassVar[int] = 32
    MAX_KEY_SIZE: ClassVar[int] = 32
    PERSON_SIZE: ClassVar[int] = 8
    SALT_SIZE: ClassVar[int] = 8
    block_size: int
    digest_size: int
    name: str
    if sys.version_info >= (3, 9):
        def __new__(
            cls,
            data: ReadableBuffer = b"",
            /,
            *,
            digest_size: int = 32,
            key: ReadableBuffer = b"",
            salt: ReadableBuffer = b"",
            person: ReadableBuffer = b"",
            fanout: int = 1,
            depth: int = 1,
            leaf_size: int = 0,
            node_offset: int = 0,
            node_depth: int = 0,
            inner_size: int = 0,
            last_node: bool = False,
            usedforsecurity: bool = True,
        ) -> Self: ...
    else:
        def __new__(
            cls,
            data: ReadableBuffer = b"",
            /,
            *,
            digest_size: int = 32,
            key: ReadableBuffer = b"",
            salt: ReadableBuffer = b"",
            person: ReadableBuffer = b"",
            fanout: int = 1,
            depth: int = 1,
            leaf_size: int = 0,
            node_offset: int = 0,
            node_depth: int = 0,
            inner_size: int = 0,
            last_node: bool = False,
        ) -> Self: ...

    def copy(self) -> Self: ...
    def digest(self) -> bytes: ...
    def hexdigest(self) -> str: ...
    def update(self, data: ReadableBuffer, /) -> None: ...
