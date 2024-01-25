class Base:
    __slots__ = ("a", "b")


class Subclass(Base):
    __slots__ = ("a", "d")  # [redefined-slots-in-subclass]
