"""Base class for MIME multipart/* type messages."""

from collections.abc import Sequence
from email import _ParamsType
from email._policybase import _MessageT
from email.mime.base import MIMEBase
from email.policy import Policy

__all__ = ["MIMEMultipart"]

class MIMEMultipart(MIMEBase):
    """Base class for MIME multipart/* type messages."""

    def __init__(
        self,
        _subtype: str = "mixed",
        boundary: str | None = None,
        _subparts: Sequence[_MessageT] | None = None,
        *,
        policy: Policy[_MessageT] | None = None,
        **_params: _ParamsType,
    ) -> None:
        """Creates a multipart/* type message.

        By default, creates a multipart/mixed message, with proper
        Content-Type and MIME-Version headers.

        _subtype is the subtype of the multipart content type, defaulting to
        'mixed'.

        boundary is the multipart boundary string.  By default it is
        calculated as needed.

        _subparts is a sequence of initial subparts for the payload.  It
        must be an iterable object, such as a list.  You can always
        attach new subparts to the message by using the attach() method.

        Additional parameters for the Content-Type header are taken from the
        keyword arguments (or passed into the _params argument).
        """
