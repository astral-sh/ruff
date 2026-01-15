"""Wrapper to the POSIX crypt library call and associated functionality."""

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
        class _Method(_MethodBase):
            """Class representing a salt method per the Modular Crypt Format or the
            legacy 2-character crypt method.
            """

    else:
        @disjoint_base
        class _Method(_MethodBase):
            """Class representing a salt method per the Modular Crypt Format or the
            legacy 2-character crypt method.
            """

    METHOD_CRYPT: Final[_Method]
    METHOD_MD5: Final[_Method]
    METHOD_SHA256: Final[_Method]
    METHOD_SHA512: Final[_Method]
    METHOD_BLOWFISH: Final[_Method]
    methods: list[_Method]
    def mksalt(method: _Method | None = None, *, rounds: int | None = None) -> str:
        """Generate a salt for the specified method.

        If not specified, the strongest available method will be used.

        """

    def crypt(word: str, salt: str | _Method | None = None) -> str:
        """Return a string representing the one-way hash of a password, with a salt
        prepended.

        If ``salt`` is not specified or is ``None``, the strongest
        available method will be selected and a salt generated.  Otherwise,
        ``salt`` may be one of the ``crypt.METHOD_*`` values, or a string as
        returned by ``crypt.mksalt()``.

        """
