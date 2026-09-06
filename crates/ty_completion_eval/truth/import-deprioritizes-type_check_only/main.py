from typing import TYPE_CHECKING

import private_stub

from module import UniquePrefixA<CURSOR:UniquePrefixAzurous>
from module import unique_prefix_<CURSOR:unique_prefix_azurous>
from private_stub import _Al<CURSOR:_Alzeta>

from module import Class

Class.meth_<CURSOR:meth_azurous>
private_stub._Al<CURSOR:_Alzeta>

# TODO: bound methods don't preserve type-check-only-ness, this is a bug
Class().meth_<CURSOR:meth_azurous>

# TODO: auto-imports don't take type-check-only-ness into account, this is a bug
UniquePrefixA<CURSOR:module.UniquePrefixAzurous>

if TYPE_CHECKING:
    from module import UniquePrefixA<CURSOR:UniquePrefixApple>
    from module import unique_prefix_<CURSOR:unique_prefix_apple>
    from private_stub import _Al<CURSOR:_Alpha>

    Class.meth_<CURSOR:meth_apple>
    private_stub._Al<CURSOR:_Alpha>

    def declared_in_type_checking_block() -> None:
        private_stub._Al<CURSOR:_Alpha>


def function_scope() -> None:
    if TYPE_CHECKING:
        from private_stub import _Al<CURSOR:_Alpha>

        private_stub._Al<CURSOR:_Alpha>


if not TYPE_CHECKING:
    pass
else:
    private_stub._Al<CURSOR:_Alpha>


if TYPE_CHECKING:
    pass
else:
    from private_stub import _Al<CURSOR:_Alzeta>
