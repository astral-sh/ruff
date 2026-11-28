import import_self.module
from import_self import module
from . import module
from import_self.module import foo
from .module import foo as f2
from import_self.module import bar, baz
from import_self.module import *
from import_self import other, module as m
from . import other


def foo(): ...
def bar(): ...
def baz(): ...
