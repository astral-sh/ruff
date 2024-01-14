from __future__ import annotations

from pandas import DataFrame
from pydantic import BaseModel


class Parent(BaseModel):
    ...


class Child(Parent):
    baz: DataFrame
