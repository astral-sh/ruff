from email._policybase import Policy
from email.mime.nonmultipart import MIMENonMultipart

__all__ = ["MIMEText"]

class MIMEText(MIMENonMultipart):
    def __init__(
        self, _text: str, _subtype: str = "plain", _charset: str | None = None, *, policy: Policy | None = None
    ) -> None: ...
