"""Base class for MIME type messages that are not multipart."""

from email.mime.base import MIMEBase

__all__ = ["MIMENonMultipart"]

class MIMENonMultipart(MIMEBase):
    """Base class for MIME non-multipart type messages."""
