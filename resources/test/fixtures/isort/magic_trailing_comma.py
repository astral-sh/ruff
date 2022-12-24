# This has a magic trailing comma, will be sorted, but not rolled into one line
from sys import (
    stderr,
    argv,
    stdout,
    exit,
)

# No magic comma, this will be rolled into one line
from os import (
    path,
    environ,
    execl,
    execv
)

from os import (
    path,
    environ,
    execl,
    execv,  # Ends with a comment, should still treat as magic trailing comma.
)

# These will be combined, but without a trailing comma.
from foo import bar
from foo import baz


# These will be combined, _with_ a trailing comma, I think? Follow `isort` here.
from foo import bar
from foo import (
  baz,
  bop,
)
