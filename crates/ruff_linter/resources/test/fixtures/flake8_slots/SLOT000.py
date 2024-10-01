class Bad(str):  # SLOT000
    pass


class Good(str):  # Ok
    __slots__ = ["foo"]


from enum import Enum


class Fine(str, Enum):  # Ok
    __slots__ = ["foo"]


class SubEnum(Enum):
    pass


class Ok(str, SubEnum):  # Ok
    pass
