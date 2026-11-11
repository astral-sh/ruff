# Errors: importing names from the current module itself.
from import_self.from_self import foo
from .from_self import foo as f2
from import_self.from_self import foo, bar
from import_self.from_self import *
from .from_self import *

# OK: importing from a sibling module or a different package.
from import_self.module import foo
from .module import foo
from import_self import foo
from other.from_self import foo


def foo(): ...


def bar(): ...
