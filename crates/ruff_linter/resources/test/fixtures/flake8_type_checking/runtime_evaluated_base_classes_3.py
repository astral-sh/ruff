from __future__ import annotations

import datetime
import pathlib
from uuid import UUID  # TCH003

import pydantic
from pydantic import BaseModel


class A(pydantic.BaseModel):
    x: datetime.datetime


class B(BaseModel):
    x: pathlib.Path


class C:
    pass


class D(C):
    x: UUID


import collections


class E(BaseModel[int]):
    x: collections.Awaitable
