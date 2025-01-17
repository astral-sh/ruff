class Base:
    __slots__ = ("a", "b")


class Subclass(Base):
    __slots__ = ("a", "d")  # [redefined-slots-in-subclass]

class Grandparent:
    __slots__ = ("a", "b")


class Parent(Grandparent):
    pass


class Child(Parent):
    __slots__ = ("c", "a")

class AnotherBase:
    __slots__ = ["a","b","c","d"]

class AnotherChild(AnotherBase):
    __slots__ = ["a","b","e","f"]
