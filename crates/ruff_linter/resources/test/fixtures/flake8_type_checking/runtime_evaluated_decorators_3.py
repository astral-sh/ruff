from __future__ import annotations

import datetime
from array import array
from dataclasses import dataclass
from uuid import UUID  # TCH003
from collections.abc import Sequence
from pydantic import validate_call

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


@validate_call(config={'arbitrary_types_allowed': True})
def test(user: Sequence):
    ...
