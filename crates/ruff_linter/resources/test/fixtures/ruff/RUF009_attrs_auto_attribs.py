import attr
from attrs import define, field, frozen, mutable


foo = int


@define  # auto_attribs = None => True
class C:
    a: str = 0
    b = field()
    c: int = foo()
    d = list()


@define()  # auto_attribs = None => True
class C:
    a: str = 0
    b = field()
    c: int = foo()
    d = list()


@define(auto_attribs=None)  # auto_attribs = None => True
class C:
    a: str = 0
    b = field()
    c: int = foo()
    d = list()


@frozen  # auto_attribs = None => True
class C:
    a: str = 0
    b = field()
    c: int = foo()
    d = list()


@frozen()  # auto_attribs = None => True
class C:
    a: str = 0
    b = field()
    c: int = foo()
    d = list()


@frozen(auto_attribs=None)  # auto_attribs = None => True
class C:
    a: str = 0
    b = field()
    c: int = foo()
    d = list()


@mutable  # auto_attribs = None => True
class C:
    a: str = 0
    b = field()
    c: int = foo()
    d = list()


@mutable()  # auto_attribs = None => True
class C:
    a: str = 0
    b = field()
    c: int = foo()
    d = list()


@mutable(auto_attribs=None)  # auto_attribs = None => True
class C:
    a: str = 0
    b = field()
    c: int = foo()
    d = list()


@attr.s  # auto_attribs = False
class C:
    a: str = 0
    b = field()
    c: int = foo()
    d = list()


@attr.s()  # auto_attribs = False
class C:
    a: str = 0
    b = field()
    c: int = foo()
    d = list()


@attr.s(auto_attribs=None)  # auto_attribs = None => True
class C:
    a: str = 0
    b = field()
    c: int = foo()
    d = list()


@attr.s(auto_attribs=False)  # auto_attribs = False
class C:
    a: str = 0
    b = field()
    c: int = foo()
    d = list()


@attr.s(auto_attribs=True)  # auto_attribs = True
class C:
    a: str = 0
    b = field()
    c: int = foo()
    d = list()


@attr.s(auto_attribs=[1, 2, 3])  # auto_attribs = False
class C:
    a: str = 0
    b = field()
    c: int = foo()
    d = list()
