"""Base class for MIME specializations."""

import email.message
from email import _ParamsType
from email.policy import Policy

__all__ = ["MIMEBase"]

class MIMEBase(email.message.Message):
    """Base class for MIME specializations."""

    def __init__(self, _maintype: str, _subtype: str, *, policy: Policy | None = None, **_params: _ParamsType) -> None:
        """This constructor adds a Content-Type: and a MIME-Version: header.

        The Content-Type: header is taken from the _maintype and _subtype
        arguments.  Additional parameters for this header are taken from the
        keyword arguments.
        """
