"""Read/write support for Maildir, mbox, MH, Babyl, and MMDF mailboxes."""

import email.message
import io
import sys
from _typeshed import StrPath, SupportsNoArgReadline, SupportsRead
from abc import ABCMeta, abstractmethod
from collections.abc import Callable, Iterable, Iterator, Mapping, Sequence
from email._policybase import _MessageT
from types import GenericAlias, TracebackType
from typing import IO, Any, AnyStr, Generic, Literal, Protocol, TypeVar, overload, type_check_only
from typing_extensions import Self, TypeAlias

__all__ = [
    "Mailbox",
    "Maildir",
    "mbox",
    "MH",
    "Babyl",
    "MMDF",
    "Message",
    "MaildirMessage",
    "mboxMessage",
    "MHMessage",
    "BabylMessage",
    "MMDFMessage",
    "Error",
    "NoSuchMailboxError",
    "NotEmptyError",
    "ExternalClashError",
    "FormatError",
]

_T = TypeVar("_T")

@type_check_only
class _SupportsReadAndReadline(SupportsRead[bytes], SupportsNoArgReadline[bytes], Protocol): ...

_MessageData: TypeAlias = email.message.Message | bytes | str | io.StringIO | _SupportsReadAndReadline

@type_check_only
class _HasIteritems(Protocol):
    def iteritems(self) -> Iterator[tuple[str, _MessageData]]: ...

@type_check_only
class _HasItems(Protocol):
    def items(self) -> Iterator[tuple[str, _MessageData]]: ...

linesep: bytes

class Mailbox(Generic[_MessageT]):
    """A group of messages in a particular place."""

    _path: str  # undocumented
    _factory: Callable[[IO[Any]], _MessageT] | None  # undocumented
    @overload
    def __init__(self, path: StrPath, factory: Callable[[IO[Any]], _MessageT], create: bool = True) -> None:
        """Initialize a Mailbox instance."""

    @overload
    def __init__(self, path: StrPath, factory: None = None, create: bool = True) -> None: ...
    @abstractmethod
    def add(self, message: _MessageData) -> str:
        """Add message and return assigned key."""

    @abstractmethod
    def remove(self, key: str) -> None:
        """Remove the keyed message; raise KeyError if it doesn't exist."""

    def __delitem__(self, key: str) -> None: ...
    def discard(self, key: str) -> None:
        """If the keyed message exists, remove it."""

    @abstractmethod
    def __setitem__(self, key: str, message: _MessageData) -> None:
        """Replace the keyed message; raise KeyError if it doesn't exist."""

    @overload
    def get(self, key: str, default: None = None) -> _MessageT | None:
        """Return the keyed message, or default if it doesn't exist."""

    @overload
    def get(self, key: str, default: _T) -> _MessageT | _T: ...
    def __getitem__(self, key: str) -> _MessageT:
        """Return the keyed message; raise KeyError if it doesn't exist."""

    @abstractmethod
    def get_message(self, key: str) -> _MessageT:
        """Return a Message representation or raise a KeyError."""

    def get_string(self, key: str) -> str:
        """Return a string representation or raise a KeyError.

        Uses email.message.Message to create a 7bit clean string
        representation of the message.
        """

    @abstractmethod
    def get_bytes(self, key: str) -> bytes:
        """Return a byte string representation or raise a KeyError."""
    # As '_ProxyFile' doesn't implement the full IO spec, and BytesIO is incompatible with it, get_file return is Any here
    @abstractmethod
    def get_file(self, key: str) -> Any:
        """Return a file-like representation or raise a KeyError."""

    @abstractmethod
    def iterkeys(self) -> Iterator[str]:
        """Return an iterator over keys."""

    def keys(self) -> list[str]:
        """Return a list of keys."""

    def itervalues(self) -> Iterator[_MessageT]:
        """Return an iterator over all messages."""

    def __iter__(self) -> Iterator[_MessageT]: ...
    def values(self) -> list[_MessageT]:
        """Return a list of messages. Memory intensive."""

    def iteritems(self) -> Iterator[tuple[str, _MessageT]]:
        """Return an iterator over (key, message) tuples."""

    def items(self) -> list[tuple[str, _MessageT]]:
        """Return a list of (key, message) tuples. Memory intensive."""

    @abstractmethod
    def __contains__(self, key: str) -> bool:
        """Return True if the keyed message exists, False otherwise."""

    @abstractmethod
    def __len__(self) -> int:
        """Return a count of messages in the mailbox."""

    def clear(self) -> None:
        """Delete all messages."""

    @overload
    def pop(self, key: str, default: None = None) -> _MessageT | None:
        """Delete the keyed message and return it, or default."""

    @overload
    def pop(self, key: str, default: _T) -> _MessageT | _T: ...
    def popitem(self) -> tuple[str, _MessageT]:
        """Delete an arbitrary (key, message) pair and return it."""

    def update(self, arg: _HasIteritems | _HasItems | Iterable[tuple[str, _MessageData]] | None = None) -> None:
        """Change the messages that correspond to certain keys."""

    @abstractmethod
    def flush(self) -> None:
        """Write any pending changes to the disk."""

    @abstractmethod
    def lock(self) -> None:
        """Lock the mailbox."""

    @abstractmethod
    def unlock(self) -> None:
        """Unlock the mailbox if it is locked."""

    @abstractmethod
    def close(self) -> None:
        """Flush and close the mailbox."""

    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """

class Maildir(Mailbox[MaildirMessage]):
    """A qmail-style Maildir mailbox."""

    colon: str
    def __init__(self, dirname: StrPath, factory: Callable[[IO[Any]], MaildirMessage] | None = None, create: bool = True) -> None:
        """Initialize a Maildir instance."""

    def add(self, message: _MessageData) -> str:
        """Add message and return assigned key."""

    def remove(self, key: str) -> None:
        """Remove the keyed message; raise KeyError if it doesn't exist."""

    def __setitem__(self, key: str, message: _MessageData) -> None:
        """Replace the keyed message; raise KeyError if it doesn't exist."""

    def get_message(self, key: str) -> MaildirMessage:
        """Return a Message representation or raise a KeyError."""

    def get_bytes(self, key: str) -> bytes:
        """Return a bytes representation or raise a KeyError."""

    def get_file(self, key: str) -> _ProxyFile[bytes]:
        """Return a file-like representation or raise a KeyError."""
    if sys.version_info >= (3, 13):
        def get_info(self, key: str) -> str:
            """Get the keyed message's "info" as a string."""

        def set_info(self, key: str, info: str) -> None:
            """Set the keyed message's "info" string."""

        def get_flags(self, key: str) -> str:
            """Return as a string the standard flags that are set on the keyed message."""

        def set_flags(self, key: str, flags: str) -> None:
            """Set the given flags and unset all others on the keyed message."""

        def add_flag(self, key: str, flag: str) -> None:
            """Set the given flag(s) without changing others on the keyed message."""

        def remove_flag(self, key: str, flag: str) -> None:
            """Unset the given string flag(s) without changing others on the keyed message."""

    def iterkeys(self) -> Iterator[str]:
        """Return an iterator over keys."""

    def __contains__(self, key: str) -> bool:
        """Return True if the keyed message exists, False otherwise."""

    def __len__(self) -> int:
        """Return a count of messages in the mailbox."""

    def flush(self) -> None:
        """Write any pending changes to disk."""

    def lock(self) -> None:
        """Lock the mailbox."""

    def unlock(self) -> None:
        """Unlock the mailbox if it is locked."""

    def close(self) -> None:
        """Flush and close the mailbox."""

    def list_folders(self) -> list[str]:
        """Return a list of folder names."""

    def get_folder(self, folder: str) -> Maildir:
        """Return a Maildir instance for the named folder."""

    def add_folder(self, folder: str) -> Maildir:
        """Create a folder and return a Maildir instance representing it."""

    def remove_folder(self, folder: str) -> None:
        """Delete the named folder, which must be empty."""

    def clean(self) -> None:
        """Delete old files in "tmp"."""

    def next(self) -> str | None:
        """Return the next message in a one-time iteration."""

class _singlefileMailbox(Mailbox[_MessageT], metaclass=ABCMeta):
    """A single-file mailbox."""

    def add(self, message: _MessageData) -> str:
        """Add message and return assigned key."""

    def remove(self, key: str) -> None:
        """Remove the keyed message; raise KeyError if it doesn't exist."""

    def __setitem__(self, key: str, message: _MessageData) -> None:
        """Replace the keyed message; raise KeyError if it doesn't exist."""

    def iterkeys(self) -> Iterator[str]:
        """Return an iterator over keys."""

    def __contains__(self, key: str) -> bool:
        """Return True if the keyed message exists, False otherwise."""

    def __len__(self) -> int:
        """Return a count of messages in the mailbox."""

    def lock(self) -> None:
        """Lock the mailbox."""

    def unlock(self) -> None:
        """Unlock the mailbox if it is locked."""

    def flush(self) -> None:
        """Write any pending changes to disk."""

    def close(self) -> None:
        """Flush and close the mailbox."""

class _mboxMMDF(_singlefileMailbox[_MessageT]):
    """An mbox or MMDF mailbox."""

    def get_message(self, key: str) -> _MessageT:
        """Return a Message representation or raise a KeyError."""

    def get_file(self, key: str, from_: bool = False) -> _PartialFile[bytes]:
        """Return a file-like representation or raise a KeyError."""

    def get_bytes(self, key: str, from_: bool = False) -> bytes:
        """Return a string representation or raise a KeyError."""

    def get_string(self, key: str, from_: bool = False) -> str:
        """Return a string representation or raise a KeyError."""

class mbox(_mboxMMDF[mboxMessage]):
    """A classic mbox mailbox."""

    def __init__(self, path: StrPath, factory: Callable[[IO[Any]], mboxMessage] | None = None, create: bool = True) -> None:
        """Initialize an mbox mailbox."""

class MMDF(_mboxMMDF[MMDFMessage]):
    """An MMDF mailbox."""

    def __init__(self, path: StrPath, factory: Callable[[IO[Any]], MMDFMessage] | None = None, create: bool = True) -> None:
        """Initialize an MMDF mailbox."""

class MH(Mailbox[MHMessage]):
    """An MH mailbox."""

    def __init__(self, path: StrPath, factory: Callable[[IO[Any]], MHMessage] | None = None, create: bool = True) -> None:
        """Initialize an MH instance."""

    def add(self, message: _MessageData) -> str:
        """Add message and return assigned key."""

    def remove(self, key: str) -> None:
        """Remove the keyed message; raise KeyError if it doesn't exist."""

    def __setitem__(self, key: str, message: _MessageData) -> None:
        """Replace the keyed message; raise KeyError if it doesn't exist."""

    def get_message(self, key: str) -> MHMessage:
        """Return a Message representation or raise a KeyError."""

    def get_bytes(self, key: str) -> bytes:
        """Return a bytes representation or raise a KeyError."""

    def get_file(self, key: str) -> _ProxyFile[bytes]:
        """Return a file-like representation or raise a KeyError."""

    def iterkeys(self) -> Iterator[str]:
        """Return an iterator over keys."""

    def __contains__(self, key: str) -> bool:
        """Return True if the keyed message exists, False otherwise."""

    def __len__(self) -> int:
        """Return a count of messages in the mailbox."""

    def flush(self) -> None:
        """Write any pending changes to the disk."""

    def lock(self) -> None:
        """Lock the mailbox."""

    def unlock(self) -> None:
        """Unlock the mailbox if it is locked."""

    def close(self) -> None:
        """Flush and close the mailbox."""

    def list_folders(self) -> list[str]:
        """Return a list of folder names."""

    def get_folder(self, folder: StrPath) -> MH:
        """Return an MH instance for the named folder."""

    def add_folder(self, folder: StrPath) -> MH:
        """Create a folder and return an MH instance representing it."""

    def remove_folder(self, folder: StrPath) -> None:
        """Delete the named folder, which must be empty."""

    def get_sequences(self) -> dict[str, list[int]]:
        """Return a name-to-key-list dictionary to define each sequence."""

    def set_sequences(self, sequences: Mapping[str, Sequence[int]]) -> None:
        """Set sequences using the given name-to-key-list dictionary."""

    def pack(self) -> None:
        """Re-name messages to eliminate numbering gaps. Invalidates keys."""

class Babyl(_singlefileMailbox[BabylMessage]):
    """An Rmail-style Babyl mailbox."""

    def __init__(self, path: StrPath, factory: Callable[[IO[Any]], BabylMessage] | None = None, create: bool = True) -> None:
        """Initialize a Babyl mailbox."""

    def get_message(self, key: str) -> BabylMessage:
        """Return a Message representation or raise a KeyError."""

    def get_bytes(self, key: str) -> bytes:
        """Return a string representation or raise a KeyError."""

    def get_file(self, key: str) -> IO[bytes]:
        """Return a file-like representation or raise a KeyError."""

    def get_labels(self) -> list[str]:
        """Return a list of user-defined labels in the mailbox."""

class Message(email.message.Message):
    """Message with mailbox-format-specific properties."""

    def __init__(self, message: _MessageData | None = None) -> None:
        """Initialize a Message instance."""

class MaildirMessage(Message):
    """Message with Maildir-specific properties."""

    def get_subdir(self) -> str:
        """Return 'new' or 'cur'."""

    def set_subdir(self, subdir: Literal["new", "cur"]) -> None:
        """Set subdir to 'new' or 'cur'."""

    def get_flags(self) -> str:
        """Return as a string the flags that are set."""

    def set_flags(self, flags: Iterable[str]) -> None:
        """Set the given flags and unset all others."""

    def add_flag(self, flag: str) -> None:
        """Set the given flag(s) without changing others."""

    def remove_flag(self, flag: str) -> None:
        """Unset the given string flag(s) without changing others."""

    def get_date(self) -> int:
        """Return delivery date of message, in seconds since the epoch."""

    def set_date(self, date: float) -> None:
        """Set delivery date of message, in seconds since the epoch."""

    def get_info(self) -> str:
        """Get the message's "info" as a string."""

    def set_info(self, info: str) -> None:
        """Set the message's "info" string."""

class _mboxMMDFMessage(Message):
    """Message with mbox- or MMDF-specific properties."""

    def get_from(self) -> str:
        """Return contents of "From " line."""

    def set_from(self, from_: str, time_: bool | tuple[int, int, int, int, int, int, int, int, int] | None = None) -> None:
        """Set "From " line, formatting and appending time_ if specified."""

    def get_flags(self) -> str:
        """Return as a string the flags that are set."""

    def set_flags(self, flags: Iterable[str]) -> None:
        """Set the given flags and unset all others."""

    def add_flag(self, flag: str) -> None:
        """Set the given flag(s) without changing others."""

    def remove_flag(self, flag: str) -> None:
        """Unset the given string flag(s) without changing others."""

class mboxMessage(_mboxMMDFMessage):
    """Message with mbox-specific properties."""

class MHMessage(Message):
    """Message with MH-specific properties."""

    def get_sequences(self) -> list[str]:
        """Return a list of sequences that include the message."""

    def set_sequences(self, sequences: Iterable[str]) -> None:
        """Set the list of sequences that include the message."""

    def add_sequence(self, sequence: str) -> None:
        """Add sequence to list of sequences including the message."""

    def remove_sequence(self, sequence: str) -> None:
        """Remove sequence from the list of sequences including the message."""

class BabylMessage(Message):
    """Message with Babyl-specific properties."""

    def get_labels(self) -> list[str]:
        """Return a list of labels on the message."""

    def set_labels(self, labels: Iterable[str]) -> None:
        """Set the list of labels on the message."""

    def add_label(self, label: str) -> None:
        """Add label to list of labels on the message."""

    def remove_label(self, label: str) -> None:
        """Remove label from the list of labels on the message."""

    def get_visible(self) -> Message:
        """Return a Message representation of visible headers."""

    def set_visible(self, visible: _MessageData) -> None:
        """Set the Message representation of visible headers."""

    def update_visible(self) -> None:
        """Update and/or sensibly generate a set of visible headers."""

class MMDFMessage(_mboxMMDFMessage):
    """Message with MMDF-specific properties."""

class _ProxyFile(Generic[AnyStr]):
    """A read-only wrapper of a file."""

    def __init__(self, f: IO[AnyStr], pos: int | None = None) -> None:
        """Initialize a _ProxyFile."""

    def read(self, size: int | None = None) -> AnyStr:
        """Read bytes."""

    def read1(self, size: int | None = None) -> AnyStr:
        """Read bytes."""

    def readline(self, size: int | None = None) -> AnyStr:
        """Read a line."""

    def readlines(self, sizehint: int | None = None) -> list[AnyStr]:
        """Read multiple lines."""

    def __iter__(self) -> Iterator[AnyStr]:
        """Iterate over lines."""

    def tell(self) -> int:
        """Return the position."""

    def seek(self, offset: int, whence: int = 0) -> None:
        """Change position."""

    def close(self) -> None:
        """Close the file."""

    def __enter__(self) -> Self:
        """Context management protocol support."""

    def __exit__(self, exc_type: type[BaseException] | None, exc: BaseException | None, tb: TracebackType | None) -> None: ...
    def readable(self) -> bool: ...
    def writable(self) -> bool: ...
    def seekable(self) -> bool: ...
    def flush(self) -> None: ...
    @property
    def closed(self) -> bool: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """

class _PartialFile(_ProxyFile[AnyStr]):
    """A read-only wrapper of part of a file."""

    def __init__(self, f: IO[AnyStr], start: int | None = None, stop: int | None = None) -> None:
        """Initialize a _PartialFile."""

class Error(Exception):
    """Raised for module-specific errors."""

class NoSuchMailboxError(Error):
    """The specified mailbox does not exist and won't be created."""

class NotEmptyError(Error):
    """The specified mailbox is not empty and deletion was requested."""

class ExternalClashError(Error):
    """Another process caused an action to fail."""

class FormatError(Error):
    """A file appears to have an invalid format."""
