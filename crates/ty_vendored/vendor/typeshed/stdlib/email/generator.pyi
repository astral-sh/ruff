"""Classes to generate plain text from a message object tree."""

from _typeshed import SupportsWrite
from email.message import Message
from email.policy import Policy
from typing import Any, Generic, TypeVar, overload
from typing_extensions import Self

__all__ = ["Generator", "DecodedGenerator", "BytesGenerator"]

# By default, generators do not have a message policy.
_MessageT = TypeVar("_MessageT", bound=Message[Any, Any], default=Any)

class Generator(Generic[_MessageT]):
    """Generates output from a Message object tree.

    This basic generator writes the message to the given file object as plain
    text.
    """

    maxheaderlen: int | None
    policy: Policy[_MessageT] | None
    @overload
    def __init__(
        self: Generator[Any],  # The Policy of the message is used.
        outfp: SupportsWrite[str],
        mangle_from_: bool | None = None,
        maxheaderlen: int | None = None,
        *,
        policy: None = None,
    ) -> None:
        """Create the generator for message flattening.

        outfp is the output file-like object for writing the message to.  It
        must have a write() method.

        Optional mangle_from_ is a flag that, when True (the default if policy
        is not set), escapes From_ lines in the body of the message by putting
        a '>' in front of them.

        Optional maxheaderlen specifies the longest length for a non-continued
        header.  When a header line is longer (in characters, with tabs
        expanded to 8 spaces) than maxheaderlen, the header will split as
        defined in the Header class.  Set maxheaderlen to zero to disable
        header wrapping.  The default is 78, as recommended (but not required)
        by RFC 5322 section 2.1.1.

        The policy keyword specifies a policy object that controls a number of
        aspects of the generator's operation.  If no policy is specified,
        the policy associated with the Message object passed to the
        flatten method is used.

        """

    @overload
    def __init__(
        self,
        outfp: SupportsWrite[str],
        mangle_from_: bool | None = None,
        maxheaderlen: int | None = None,
        *,
        policy: Policy[_MessageT],
    ) -> None: ...
    def write(self, s: str) -> None: ...
    def flatten(self, msg: _MessageT, unixfrom: bool = False, linesep: str | None = None) -> None:
        """Print the message object tree rooted at msg to the output file
        specified when the Generator instance was created.

        unixfrom is a flag that forces the printing of a Unix From_ delimiter
        before the first object in the message tree.  If the original message
        has no From_ delimiter, a 'standard' one is crafted.  By default, this
        is False to inhibit the printing of any From_ delimiter.

        Note that for subobjects, no From_ line is printed.

        linesep specifies the characters used to indicate a new line in
        the output.  The default value is determined by the policy specified
        when the Generator instance was created or, if none was specified,
        from the policy associated with the msg.

        """

    def clone(self, fp: SupportsWrite[str]) -> Self:
        """Clone this generator with the exact same options."""

class BytesGenerator(Generator[_MessageT]):
    """Generates a bytes version of a Message object tree.

    Functionally identical to the base Generator except that the output is
    bytes and not string.  When surrogates were used in the input to encode
    bytes, these are decoded back to bytes for output.  If the policy has
    cte_type set to 7bit, then the message is transformed such that the
    non-ASCII bytes are properly content transfer encoded, using the charset
    unknown-8bit.

    The outfp object must accept bytes in its write method.
    """

    @overload
    def __init__(
        self: BytesGenerator[Any],  # The Policy of the message is used.
        outfp: SupportsWrite[bytes],
        mangle_from_: bool | None = None,
        maxheaderlen: int | None = None,
        *,
        policy: None = None,
    ) -> None:
        """Create the generator for message flattening.

        outfp is the output file-like object for writing the message to.  It
        must have a write() method.

        Optional mangle_from_ is a flag that, when True (the default if policy
        is not set), escapes From_ lines in the body of the message by putting
        a '>' in front of them.

        Optional maxheaderlen specifies the longest length for a non-continued
        header.  When a header line is longer (in characters, with tabs
        expanded to 8 spaces) than maxheaderlen, the header will split as
        defined in the Header class.  Set maxheaderlen to zero to disable
        header wrapping.  The default is 78, as recommended (but not required)
        by RFC 5322 section 2.1.1.

        The policy keyword specifies a policy object that controls a number of
        aspects of the generator's operation.  If no policy is specified,
        the policy associated with the Message object passed to the
        flatten method is used.

        """

    @overload
    def __init__(
        self,
        outfp: SupportsWrite[bytes],
        mangle_from_: bool | None = None,
        maxheaderlen: int | None = None,
        *,
        policy: Policy[_MessageT],
    ) -> None: ...

class DecodedGenerator(Generator[_MessageT]):
    """Generates a text representation of a message.

    Like the Generator base class, except that non-text parts are substituted
    with a format string representing the part.
    """

    @overload
    def __init__(
        self: DecodedGenerator[Any],  # The Policy of the message is used.
        outfp: SupportsWrite[str],
        mangle_from_: bool | None = None,
        maxheaderlen: int | None = None,
        fmt: str | None = None,
        *,
        policy: None = None,
    ) -> None:
        """Like Generator.__init__() except that an additional optional
        argument is allowed.

        Walks through all subparts of a message.  If the subpart is of main
        type 'text', then it prints the decoded payload of the subpart.

        Otherwise, fmt is a format string that is used instead of the message
        payload.  fmt is expanded with the following keywords (in
        %(keyword)s format):

        type       : Full MIME type of the non-text part
        maintype   : Main MIME type of the non-text part
        subtype    : Sub-MIME type of the non-text part
        filename   : Filename of the non-text part
        description: Description associated with the non-text part
        encoding   : Content transfer encoding of the non-text part

        The default value for fmt is None, meaning

        [Non-text (%(type)s) part of message omitted, filename %(filename)s]
        """

    @overload
    def __init__(
        self,
        outfp: SupportsWrite[str],
        mangle_from_: bool | None = None,
        maxheaderlen: int | None = None,
        fmt: str | None = None,
        *,
        policy: Policy[_MessageT],
    ) -> None: ...
