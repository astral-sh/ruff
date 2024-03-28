from a import b  # comment
from . import a
from foo.bar import baz as b, FooBar as fb
from .a import b
from ... import c
from .......................... import d
from ..........................a.b.c import d
from module import (a, b as B, c,)
from a import *
