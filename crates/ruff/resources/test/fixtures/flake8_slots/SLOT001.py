class Bad(tuple):  # SLOT001
    pass


class Good(tuple):  # Ok
    __slots__ = ("foo",)
