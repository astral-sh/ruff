"""Class representing text/* type MIME documents."""

from email._policybase import Policy
from email.mime.nonmultipart import MIMENonMultipart

__all__ = ["MIMEText"]

class MIMEText(MIMENonMultipart):
    """Class for generating text/* type MIME documents."""

    def __init__(self, _text: str, _subtype: str = "plain", _charset: str | None = None, *, policy: Policy | None = None) -> None:
        """Create a text/* type MIME document.

        _text is the string for this message object.

        _subtype is the MIME sub content type, defaulting to "plain".

        _charset is the character set parameter added to the Content-Type
        header.  This defaults to "us-ascii".  Note that as a side-effect, the
        Content-Transfer-Encoding header will also be set.
        """
