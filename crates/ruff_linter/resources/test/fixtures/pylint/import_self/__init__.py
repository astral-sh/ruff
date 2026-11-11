# OK: in a package `__init__.py`, `from <package> import <name>` usually imports a
# submodule, which can't be distinguished from importing a name defined in this file.
from import_self import module
from import_self import foo
