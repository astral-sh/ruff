"""Test: parsing of nested string annotations."""

from typing import List
from pathlib import Path, PurePath


x: """List['Path']""" = []
