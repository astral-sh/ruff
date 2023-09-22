from __future__ import annotations

import datetime
from array import array
from dataclasses import dataclass
from uuid import UUID  # TCH003

import attrs
from attrs import frozen


@attrs.define(auto_attribs=True)
class A:
    x: datetime.datetime


@frozen
class B:
    x: array


@dataclass
class C:
    x: UUID
