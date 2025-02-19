# setup
from enum import Enum, EnumMeta
from collections import UserList as UL

class SetOnceMappingMixin:
    __slots__ = ()
    def __setitem__(self, key, value):
        if key in self:
            raise KeyError(str(key) + ' already set')
        return super().__setitem__(key, value)


class CaseInsensitiveEnumMeta(EnumMeta):
    pass

# positives
class D(dict):
    pass

class L(list):
    pass

class S(str):
    pass

class SubscriptDict(dict[str, str]):
    pass

class SubscriptList(list[str]):
    pass

# currently not detected
class SetOnceDict(SetOnceMappingMixin, dict):
    pass

# negatives
class C:
    pass

class I(int):
    pass

class ActivityState(str, Enum, metaclass=CaseInsensitiveEnumMeta):
    """Activity state. This is an optional property and if not provided, the state will be Active by
    default.
    """
    ACTIVE = "Active"
    INACTIVE = "Inactive"
