"""Parse a Python module and describe its classes and functions.

Parse enough of a Python file to recognize imports and class and
function definitions, and to find out the superclasses of a class.

The interface consists of a single function:
    readmodule_ex(module, path=None)
where module is the name of a Python module, and path is an optional
list of directories where the module is to be searched.  If present,
path is prepended to the system search path sys.path.  The return value
is a dictionary.  The keys of the dictionary are the names of the
classes and functions defined in the module (including classes that are
defined via the from XXX import YYY construct).  The values are
instances of classes Class and Function.  One special key/value pair is
present for packages: the key '__path__' has a list as its value which
contains the package search path.

Classes and Functions have a common superclass: _Object.  Every instance
has the following attributes:
    module  -- name of the module;
    name    -- name of the object;
    file    -- file in which the object is defined;
    lineno  -- line in the file where the object's definition starts;
    end_lineno -- line in the file where the object's definition ends;
    parent  -- parent of this object, if any;
    children -- nested objects contained in this object.
The 'children' attribute is a dictionary mapping names to objects.

Instances of Function describe functions with the attributes from _Object,
plus the following:
    is_async -- if a function is defined with an 'async' prefix

Instances of Class describe classes with the attributes from _Object,
plus the following:
    super   -- list of super classes (Class instances if possible);
    methods -- mapping of method names to beginning line numbers.
If the name of a super class is not recognized, the corresponding
entry in the list of super classes is not a class instance but a
string giving the name of the super class.  Since import statements
are recognized and imported modules are scanned as well, this
shouldn't happen often.
"""

import sys
from collections.abc import Mapping, Sequence

__all__ = ["readmodule", "readmodule_ex", "Class", "Function"]

class _Object:
    """Information about Python class or function."""

    module: str
    name: str
    file: int
    lineno: int

    if sys.version_info >= (3, 10):
        end_lineno: int | None

    parent: _Object | None

    # This is a dict at runtime, but we're typing it as Mapping to
    # avoid variance issues in the subclasses
    children: Mapping[str, _Object]

    if sys.version_info >= (3, 10):
        def __init__(
            self, module: str, name: str, file: str, lineno: int, end_lineno: int | None, parent: _Object | None
        ) -> None: ...
    else:
        def __init__(self, module: str, name: str, file: str, lineno: int, parent: _Object | None) -> None: ...

class Function(_Object):
    """Information about a Python function, including methods."""

    if sys.version_info >= (3, 10):
        is_async: bool

    parent: Function | Class | None
    children: dict[str, Class | Function]

    if sys.version_info >= (3, 10):
        def __init__(
            self,
            module: str,
            name: str,
            file: str,
            lineno: int,
            parent: Function | Class | None = None,
            is_async: bool = False,
            *,
            end_lineno: int | None = None,
        ) -> None: ...
    else:
        def __init__(self, module: str, name: str, file: str, lineno: int, parent: Function | Class | None = None) -> None: ...

class Class(_Object):
    """Information about a Python class."""

    super: list[Class | str] | None
    methods: dict[str, int]
    parent: Class | None
    children: dict[str, Class | Function]

    if sys.version_info >= (3, 10):
        def __init__(
            self,
            module: str,
            name: str,
            super_: list[Class | str] | None,
            file: str,
            lineno: int,
            parent: Class | None = None,
            *,
            end_lineno: int | None = None,
        ) -> None: ...
    else:
        def __init__(
            self, module: str, name: str, super: list[Class | str] | None, file: str, lineno: int, parent: Class | None = None
        ) -> None: ...

def readmodule(module: str, path: Sequence[str] | None = None) -> dict[str, Class]:
    """Return Class objects for the top-level classes in module.

    This is the original interface, before Functions were added.
    """

def readmodule_ex(module: str, path: Sequence[str] | None = None) -> dict[str, Class | Function | list[str]]:
    """Return a dictionary with all functions and classes in module.

    Search for module in PATH + sys.path.
    If possible, include imported superclasses.
    Do this by reading source, without importing (and executing) it.
    """
