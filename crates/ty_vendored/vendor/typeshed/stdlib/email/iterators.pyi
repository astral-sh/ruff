"""Various types of useful iterators and generators."""

from _typeshed import SupportsWrite
from collections.abc import Iterator
from email.message import Message

__all__ = ["body_line_iterator", "typed_subpart_iterator", "walk"]

def body_line_iterator(msg: Message, decode: bool = False) -> Iterator[str]:
    """Iterate over the parts, returning string payloads line-by-line.

    Optional decode (default False) is passed through to .get_payload().
    """

def typed_subpart_iterator(msg: Message, maintype: str = "text", subtype: str | None = None) -> Iterator[str]:
    """Iterate over the subparts with a given MIME type.

    Use 'maintype' as the main MIME type to match against; this defaults to
    "text".  Optional 'subtype' is the MIME subtype to match against; if
    omitted, only the main type is matched.
    """

def walk(self: Message) -> Iterator[Message]:
    """Walk over the message tree, yielding each subpart.

    The walk is performed in depth-first order.  This method is a
    generator.
    """

# We include the seemingly private function because it is documented in the stdlib documentation.
def _structure(msg: Message, fp: SupportsWrite[str] | None = None, level: int = 0, include_default: bool = False) -> None:
    """A handy debugging aid"""
