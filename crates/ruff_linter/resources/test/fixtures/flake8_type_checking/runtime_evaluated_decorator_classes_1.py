from __future__ import annotations

from typing import TYPE_CHECKING

import fastapi
from fastapi import FastAPI as Api

from example import DecoratingClass

if TYPE_CHECKING:
    import datetime  # TC004
    from array import array  # TC004

    import pathlib  # TC004

    import pyproj

app1 = fastapi.FastAPI("First application")
app2 = Api("Second application")

decorating_instance = DecoratingClass()

@app1.put("/datetime")
def set_datetime(value: datetime.datetime):
    pass

@app2.get("/array")
def get_array() -> array:
    pass

@decorating_instance.decorator
def foo(path: pathlib.Path) -> None:
    pass

@decorating_instance
def bar(arg: pyproj.Transformer) -> None:
    pass

@DecoratingClass
def baz(arg: pyproj.Transformer) -> None:
    pass
