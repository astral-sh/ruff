"""Class representing application/* type MIME documents."""

from collections.abc import Callable
from email import _ParamsType
from email.mime.nonmultipart import MIMENonMultipart
from email.policy import Policy

__all__ = ["MIMEApplication"]

class MIMEApplication(MIMENonMultipart):
    """Class for generating application/* MIME documents."""

    def __init__(
        self,
        _data: str | bytes | bytearray,
        _subtype: str = "octet-stream",
        _encoder: Callable[[MIMEApplication], object] = ...,
        *,
        policy: Policy | None = None,
        **_params: _ParamsType,
    ) -> None:
        """Create an application/* type MIME document.

        _data contains the bytes for the raw application data.

        _subtype is the MIME content type subtype, defaulting to
        'octet-stream'.

        _encoder is a function which will perform the actual encoding for
        transport of the application data, defaulting to base64 encoding.

        Any additional keyword arguments are passed to the base class
        constructor, which turns them into parameters on the Content-Type
        header.
        """
