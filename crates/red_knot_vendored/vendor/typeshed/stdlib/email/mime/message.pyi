from email.mime.nonmultipart import MIMENonMultipart
from email.policy import Policy, _MessageT

__all__ = ["MIMEMessage"]

class MIMEMessage(MIMENonMultipart):
    def __init__(self, _msg: _MessageT, _subtype: str = "rfc822", *, policy: Policy[_MessageT] | None = None) -> None: ...
