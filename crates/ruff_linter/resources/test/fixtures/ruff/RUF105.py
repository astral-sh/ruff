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
