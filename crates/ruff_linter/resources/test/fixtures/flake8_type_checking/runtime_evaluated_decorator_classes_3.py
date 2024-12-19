from __future__ import annotations

import pathlib

import fastapi
from fastapi import FastAPI as Api
from example import DecoratingClass

from uuid import UUID  # TC003

app1 = fastapi.FastAPI("First application")
app2 = Api("Second application")

decorating_instance = DecoratingClass()


@app1.get("/path")
def get_path() -> pathlib.Path:
    pass

@app2.put("/pure_path")
def set_pure_path(df: pathlib.PurePath):
    pass

@decorating_instance
def foo(x: pathlib.PosixPath):
    pass

@DecoratingClass
def bar(x: UUID):
    pass
