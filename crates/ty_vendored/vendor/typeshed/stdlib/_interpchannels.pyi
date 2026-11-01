"""This module provides primitive operations to manage Python interpreters.
The 'interpreters' module provides a more convenient interface.
"""

from _typeshed import structseq
from typing import Any, Final, Literal, SupportsIndex, final
from typing_extensions import Buffer, Self

class ChannelError(RuntimeError): ...
class ChannelClosedError(ChannelError): ...
class ChannelEmptyError(ChannelError): ...
class ChannelNotEmptyError(ChannelError): ...
class ChannelNotFoundError(ChannelError): ...

# Mark as final, since instantiating ChannelID is not supported.
@final
class ChannelID:
    """A channel ID identifies a channel and may be used as an int."""

    @property
    def end(self) -> Literal["send", "recv", "both"]:
        """'send', 'recv', or 'both'"""

    @property
    def send(self) -> Self:
        """the 'send' end of the channel"""

    @property
    def recv(self) -> Self:
        """the 'recv' end of the channel"""

    def __eq__(self, other: object, /) -> bool: ...
    def __ge__(self, other: ChannelID, /) -> bool: ...
    def __gt__(self, other: ChannelID, /) -> bool: ...
    def __hash__(self) -> int: ...
    def __index__(self) -> int:
        """Return self converted to an integer, if self is suitable for use as an index into a list."""

    def __int__(self) -> int:
        """int(self)"""

    def __le__(self, other: ChannelID, /) -> bool: ...
    def __lt__(self, other: ChannelID, /) -> bool: ...
    def __ne__(self, other: object, /) -> bool: ...

@final
class ChannelInfo(structseq[int], tuple[bool, bool, bool, int, int, int, int, int]):
    """ChannelInfo

    A named tuple of a channel's state.
    """

    __match_args__: Final = (
        "open",
        "closing",
        "closed",
        "count",
        "num_interp_send",
        "num_interp_send_released",
        "num_interp_recv",
        "num_interp_recv_released",
    )
    @property
    def open(self) -> bool:
        """both ends are open"""

    @property
    def closing(self) -> bool:
        """send is closed, recv is non-empty"""

    @property
    def closed(self) -> bool:
        """both ends are closed"""

    @property
    def count(self) -> int:  # type: ignore[override]
        """queued objects"""

    @property
    def num_interp_send(self) -> int:
        """interpreters bound to the send end"""

    @property
    def num_interp_send_released(self) -> int:
        """interpreters bound to the send end and released"""

    @property
    def num_interp_recv(self) -> int:
        """interpreters bound to the send end"""

    @property
    def num_interp_recv_released(self) -> int:
        """interpreters bound to the send end and released"""

    @property
    def num_interp_both(self) -> int:
        """interpreters bound to both ends"""

    @property
    def num_interp_both_recv_released(self) -> int:
        """interpreters bound to both ends and released_from_the recv end"""

    @property
    def num_interp_both_send_released(self) -> int:
        """interpreters bound to both ends and released_from_the send end"""

    @property
    def num_interp_both_released(self) -> int:
        """interpreters bound to both ends and released_from_both"""

    @property
    def recv_associated(self) -> bool:
        """current interpreter is bound to the recv end"""

    @property
    def recv_released(self) -> bool:
        """current interpreter *was* bound to the recv end"""

    @property
    def send_associated(self) -> bool:
        """current interpreter is bound to the send end"""

    @property
    def send_released(self) -> bool:
        """current interpreter *was* bound to the send end"""

def create(unboundop: Literal[1, 2, 3]) -> ChannelID:
    """channel_create(unboundop) -> cid

    Create a new cross-interpreter channel and return a unique generated ID.
    """

def destroy(cid: SupportsIndex) -> None:
    """channel_destroy(cid)

    Close and finalize the channel.  Afterward attempts to use the channel
    will behave as though it never existed.
    """

def list_all() -> list[ChannelID]:
    """channel_list_all() -> [cid]

    Return the list of all IDs for active channels.
    """

def list_interpreters(cid: SupportsIndex, *, send: bool) -> list[int]:
    """channel_list_interpreters(cid, *, send) -> [id]

    Return the list of all interpreter IDs associated with an end of the channel.

    The 'send' argument should be a boolean indicating whether to use the send or
    receive end.
    """

def send(cid: SupportsIndex, obj: object, *, blocking: bool = True, timeout: float | None = None) -> None:
    """channel_send(cid, obj, *, blocking=True, timeout=None)

    Add the object's data to the channel's queue.
    By default this waits for the object to be received.
    """

def send_buffer(cid: SupportsIndex, obj: Buffer, *, blocking: bool = True, timeout: float | None = None) -> None:
    """channel_send_buffer(cid, obj, *, blocking=True, timeout=None)

    Add the object's buffer to the channel's queue.
    By default this waits for the object to be received.
    """

def recv(cid: SupportsIndex, default: object = ...) -> tuple[Any, Literal[1, 2, 3]]:
    """channel_recv(cid, [default]) -> (obj, unboundop)

    Return a new object from the data at the front of the channel's queue.

    If there is nothing to receive then raise ChannelEmptyError, unless
    a default value is provided.  In that case return it.
    """

def close(cid: SupportsIndex, *, send: bool = False, recv: bool = False) -> None:
    """channel_close(cid, *, send=None, recv=None, force=False)

    Close the channel for all interpreters.

    If the channel is empty then the keyword args are ignored and both
    ends are immediately closed.  Otherwise, if 'force' is True then
    all queued items are released and both ends are immediately
    closed.

    If the channel is not empty *and* 'force' is False then following
    happens:

     * recv is True (regardless of send):
       - raise ChannelNotEmptyError
     * recv is None and send is None:
       - raise ChannelNotEmptyError
     * send is True and recv is not True:
       - fully close the 'send' end
       - close the 'recv' end to interpreters not already receiving
       - fully close it once empty

    Closing an already closed channel results in a ChannelClosedError.

    Once the channel's ID has no more ref counts in any interpreter
    the channel will be destroyed.
    """

def get_count(cid: SupportsIndex) -> int:
    """get_count(cid)

    Return the number of items in the channel.
    """

def get_info(cid: SupportsIndex) -> ChannelInfo:
    """get_info(cid)

    Return details about the channel.
    """

def get_channel_defaults(cid: SupportsIndex) -> Literal[1, 2, 3]:
    """get_channel_defaults(cid)

    Return the channel's default values, set when it was created.
    """

def release(cid: SupportsIndex, *, send: bool = False, recv: bool = False, force: bool = False) -> None:
    """channel_release(cid, *, send=None, recv=None, force=True)

    Close the channel for the current interpreter.  'send' and 'recv'
    (bool) may be used to indicate the ends to close.  By default both
    ends are closed.  Closing an already closed end is a noop.
    """
