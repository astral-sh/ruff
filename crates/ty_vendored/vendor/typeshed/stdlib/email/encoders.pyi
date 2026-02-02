"""Encodings and related functions."""

from email.message import Message

__all__ = ["encode_7or8bit", "encode_base64", "encode_noop", "encode_quopri"]

def encode_base64(msg: Message) -> None:
    """Encode the message's payload in Base64.

    Also, add an appropriate Content-Transfer-Encoding header.
    """

def encode_quopri(msg: Message) -> None:
    """Encode the message's payload in quoted-printable.

    Also, add an appropriate Content-Transfer-Encoding header.
    """

def encode_7or8bit(msg: Message) -> None:
    """Set the Content-Transfer-Encoding header to 7bit or 8bit."""

def encode_noop(msg: Message) -> None:
    """Do nothing."""
