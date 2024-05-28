import contextlib
import pathlib
import pathlib as pl
from pathlib import Path
from pathlib import Path as P

# SIM115
f = open("foo.txt")
f = Path("foo.txt").open()
f = pathlib.Path("foo.txt").open()
f = pl.Path("foo.txt").open()
f = P("foo.txt").open()
data = f.read()
f.close()

# OK
with open("foo.txt") as f:
    data = f.read()

# OK
with contextlib.ExitStack() as exit_stack:
    f = exit_stack.enter_context(open("filename"))

# OK
with contextlib.ExitStack() as stack:
    files = [stack.enter_context(open(fname)) for fname in filenames]
    close_files = stack.pop_all().close

# OK
with contextlib.AsyncExitStack() as exit_stack:
    f = await exit_stack.enter_async_context(open("filename"))

# OK (false negative)
with contextlib.ExitStack():
    f = exit_stack.enter_context(open("filename"))

# SIM115
with contextlib.ExitStack():
    f = open("filename")

# OK
with contextlib.ExitStack() as exit_stack:
    exit_stack_ = exit_stack
    f = exit_stack_.enter_context(open("filename"))

# OK (quick one-liner to clear file contents)
open("filename", "w").close()
pathlib.Path("filename").open("w").close()


# OK (custom context manager)
class MyFile:
    def __init__(self, filename: str):
        self.filename = filename

    def __enter__(self):
        self.file = open(self.filename)

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.file.close()
