from __future__ import annotations

import pathlib
from typing import TYPE_CHECKING

import attrs
from attrs import frozen

import numpy

if TYPE_CHECKING:
    import datetime  # TC004
    from array import array  # TC004

    import pandas  # TC004
    import pyproj


@attrs.define(auto_attribs=True)
class A:
    x: datetime.datetime


@attrs.define
class B:
    x: pandas.DataFrame


@frozen(auto_attribs=True)
class C:
    x: pathlib.Path


@frozen
class D:
    x: array


@dataclass
class E:
    x: pyproj.Transformer


@attrs.define
class F:
    x: numpy.ndarray
