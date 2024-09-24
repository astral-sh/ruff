import sys
from typing import Final

if sys.platform != "win32":
    class _Method: ...
    METHOD_CRYPT: Final[_Method]
    METHOD_MD5: Final[_Method]
    METHOD_SHA256: Final[_Method]
    METHOD_SHA512: Final[_Method]
    METHOD_BLOWFISH: Final[_Method]
    methods: list[_Method]
    def mksalt(method: _Method | None = None, *, rounds: int | None = None) -> str: ...
    def crypt(word: str, salt: str | _Method | None = None) -> str: ...
