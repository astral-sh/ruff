"""_blake2b provides BLAKE2b for hashlib"""

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
    """Return a new BLAKE2b hash object."""

    MAX_DIGEST_SIZE: ClassVar[int] = 64
    MAX_KEY_SIZE: ClassVar[int] = 64
    PERSON_SIZE: ClassVar[int] = 16
    SALT_SIZE: ClassVar[int] = 16
    block_size: int
    digest_size: int
    name: str
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
    def copy(self) -> Self:
        """Return a copy of the hash object."""

    def digest(self) -> bytes:
        """Return the digest value as a bytes object."""

    def hexdigest(self) -> str:
        """Return the digest value as a string of hexadecimal digits."""

    def update(self, data: ReadableBuffer, /) -> None:
        """Update this hash object's state with the provided bytes-like object."""

@final
class blake2s:
    """Return a new BLAKE2s hash object."""

    MAX_DIGEST_SIZE: ClassVar[int] = 32
    MAX_KEY_SIZE: ClassVar[int] = 32
    PERSON_SIZE: ClassVar[int] = 8
    SALT_SIZE: ClassVar[int] = 8
    block_size: int
    digest_size: int
    name: str
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
    def copy(self) -> Self:
        """Return a copy of the hash object."""

    def digest(self) -> bytes:
        """Return the digest value as a bytes object."""

    def hexdigest(self) -> str:
        """Return the digest value as a string of hexadecimal digits."""

    def update(self, data: ReadableBuffer, /) -> None:
        """Update this hash object's state with the provided bytes-like object."""
