from module import UniquePrefixA<CURSOR:UniquePrefixAzurous>
from module import unique_prefix_<CURSOR:unique_prefix_azurous>

from module import Class

Class.meth_<CURSOR:meth_azurous>

# TODO: bound methods don't preserve type-check-only-ness, this is a bug
Class().meth_<CURSOR:meth_azurous>

# TODO: auto-imports don't take type-check-only-ness into account, this is a bug
UniquePrefixA<CURSOR:module.UniquePrefixAzurous>
