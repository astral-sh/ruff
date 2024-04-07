def function(
    posonly_nohint,
    posonly_nonboolhint: int,
    posonly_boolhint: bool,
    posonly_boolstrhint: "bool",
    /,
    offset,
    posorkw_nonvalued_nohint,
    posorkw_nonvalued_nonboolhint: int,
    posorkw_nonvalued_boolhint: bool,
    posorkw_nonvalued_boolstrhint: "bool",
    posorkw_boolvalued_nohint=True,
    posorkw_boolvalued_nonboolhint: int = True,
    posorkw_boolvalued_boolhint: bool = True,
    posorkw_boolvalued_boolstrhint: "bool" = True,
    posorkw_nonboolvalued_nohint=1,
    posorkw_nonboolvalued_nonboolhint: int = 2,
    posorkw_nonboolvalued_boolhint: bool = 3,
    posorkw_nonboolvalued_boolstrhint: "bool" = 4,
    *,
    kwonly_nonvalued_nohint,
    kwonly_nonvalued_nonboolhint: int,
    kwonly_nonvalued_boolhint: bool,
    kwonly_nonvalued_boolstrhint: "bool",
    kwonly_boolvalued_nohint=True,
    kwonly_boolvalued_nonboolhint: int = False,
    kwonly_boolvalued_boolhint: bool = True,
    kwonly_boolvalued_boolstrhint: "bool" = True,
    kwonly_nonboolvalued_nohint=5,
    kwonly_nonboolvalued_nonboolhint: int = 1,
    kwonly_nonboolvalued_boolhint: bool = 1,
    kwonly_nonboolvalued_boolstrhint: "bool" = 1,
    **kw,
): ...


def used(do):
    return do


used("a", True)
used(do=True)


# Avoid FBT003 for explicitly allowed methods.
"""
FBT003 Boolean positional value on dict
"""
a = {"a": "b"}
a.get("hello", False)
{}.get("hello", False)
{}.setdefault("hello", True)
{}.pop("hello", False)
{}.pop(True, False)
dict.fromkeys(("world",), True)
{}.deploy(True, False)
getattr(someobj, attrname, False)
mylist.index(True)
bool(False)
int(True)
str(int(False))
cfg.get("hello", True)
cfg.getint("hello", True)
cfg.getfloat("hello", True)
cfg.getboolean("hello", True)
os.set_blocking(0, False)
g_action.set_enabled(True)
settings.set_enable_developer_extras(True)
foo.is_(True)
bar.is_not(False)
next(iter([]), False)
sa.func.coalesce(tbl.c.valid, False)
setVisible(True)
set_visible(True)


class Registry:
    def __init__(self) -> None:
        self._switches = [False] * len(Switch)

    # FBT001: Boolean positional arg in function definition
    def __setitem__(self, switch: Switch, value: bool) -> None:
        self._switches[switch.value] = value

    @foo.setter
    def foo(self, value: bool) -> None:
        pass

    # FBT001: Boolean positional arg in function definition
    def foo(self, value: bool) -> None:
        pass

    def foo(self) -> None:
        object.__setattr__(self, "flag", True)


from typing import Optional, Union


def func(x: Union[list, Optional[int | str | float | bool]]):
    pass


def func(x: bool | str):
    pass


def func(x: int | str):
    pass


from typing import override


@override
def func(x: bool):
    pass


settings(True)


from dataclasses import dataclass, InitVar


@dataclass
class Fit:
    force: InitVar[bool] = False

    def __post_init__(self, force: bool) -> None:
        print(force)


Fit(force=True)


# https://github.com/astral-sh/ruff/issues/10356
from django.db.models import Case, Q, Value, When


qs.annotate(
    is_foo_or_bar=Case(
        When(Q(is_foo=True) | Q(is_bar=True)),
        then=Value(True),
    ),
    default=Value(False),
)


# https://github.com/astral-sh/ruff/issues/10485
from pydantic import Field
from pydantic_settings import BaseSettings


class Settings(BaseSettings):
    foo: bool = Field(True, exclude=True)
