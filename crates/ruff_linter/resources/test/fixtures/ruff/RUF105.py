# import module => ok
import stat
print(stat.filemode(0o666))

# import type => not ok
from pathlib import Path
path = Path('/')

# import module, function => ok, not ok
from os import path, dup
cwd = path.cwd()
fd = dup(0)

# import function, but not use it => let's pretend it's ok
from sys import exit

# import type => not ok
from subprocess import CompletedProcess
def assert_success(cp: CompletedProcess):
    assert cp.returncode == 0

# import type => not ok
from argparse import ArgumentError
class MyError(ArgumentError):
    pass

# import type from typing => ok
from typing import Any
def func(arg: Any):
    pass

# import function from six.moves => ok
import sys
from six.moves import reload_module
reload_module(sys)

# import function from typing_extensions => ok
from typing import Literal
from typing_extensions import get_origin
get_origin(Literal[42])

# import type from collections.abc => ok
from collections.abc import Set
class ListBasedSet(Set):
    pass
