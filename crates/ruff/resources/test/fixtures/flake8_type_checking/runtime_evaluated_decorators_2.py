from __future__ import annotations

from dataclasses import dataclass

import attrs
import pandas
import pyproj
from attrs import frozen

import numpy  # TCH002


@attrs.define(auto_attribs=True)
class A:
    x: pyproj.Transformer


@attrs.define
class B:
    x: pandas.DataFrame


@frozen
class C:
    x: pandas.DataFrame


@dataclass
class D:
    x: numpy.ndarray
