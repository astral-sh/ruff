class Bad(str):  # SLOT000
    pass


class Good(str):  # Ok
    __slots__ = ["foo"]
