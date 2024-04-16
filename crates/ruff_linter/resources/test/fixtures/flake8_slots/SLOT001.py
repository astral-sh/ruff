class Bad(tuple):  # SLOT001
    pass


class Good(tuple):  # Ok
    __slots__ = ("foo",)


from typing import Tuple


class Bad(Tuple):  # SLOT001
    pass


class Bad(Tuple[str, int, float]):  # SLOT001
    pass


class Good(Tuple[str, int, float]):  # OK
    __slots__ = ("foo",)


import builtins

class AlsoBad(builtins.tuple[int, int]):  # SLOT001
    pass
