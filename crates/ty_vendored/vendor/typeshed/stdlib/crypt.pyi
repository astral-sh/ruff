import sys
from typing import Final, NamedTuple, type_check_only
from typing_extensions import disjoint_base

if sys.platform != "win32":
    @type_check_only
    class _MethodBase(NamedTuple):
        name: str
        ident: str | None
        salt_chars: int
        total_size: int

    if sys.version_info >= (3, 12):
        class _Method(_MethodBase): ...
    else:
        @disjoint_base
        class _Method(_MethodBase): ...

    METHOD_CRYPT: Final[_Method]
    METHOD_MD5: Final[_Method]
    METHOD_SHA256: Final[_Method]
    METHOD_SHA512: Final[_Method]
    METHOD_BLOWFISH: Final[_Method]
    methods: list[_Method]
    def mksalt(method: _Method | None = None, *, rounds: int | None = None) -> str: ...
    def crypt(word: str, salt: str | _Method | None = None) -> str: ...
