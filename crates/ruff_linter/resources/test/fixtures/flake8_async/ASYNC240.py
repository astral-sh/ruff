import os
from typing import Optional
from pathlib import Path

## Valid cases:

def os_path_in_foo():
    file = "file.txt"

    os.path.abspath(file) # OK
    os.path.exists(file) # OK
    os.path.split() # OK

async def non_io_os_path_methods():
    os.path.split() # OK
    os.path.dirname() # OK
    os.path.basename() # OK
    os.path.join() # OK

def pathlib_path_in_foo():
    path = Path("src/my_text.txt") # OK
    path.exists() # OK
    with path.open() as f: # OK
        ...
    path = Path("src/my_text.txt").open() # OK

async def non_io_pathlib_path_methods():
    path = Path("src/my_text.txt")
    path.is_absolute() # OK
    path.is_relative_to() # OK
    path.as_posix() # OK
    path.relative_to() # OK

def inline_path_method_call():
    Path("src/my_text.txt").open() # OK
    Path("src/my_text.txt").open().flush() # OK
    with Path("src/my_text.txt").open() as f: # OK
        ...

async def trio_path_in_foo():
    from trio import Path

    path = Path("src/my_text.txt") # OK
    await path.absolute() # OK
    await path.exists() # OK
    with Path("src/my_text.txt").open() as f: # OK
        ...

async def anyio_path_in_foo():
    from anyio import Path

    path = Path("src/my_text.txt") # OK
    await path.absolute() # OK
    await path.exists() # OK
    with Path("src/my_text.txt").open() as f: # OK
        ...

async def path_open_in_foo():
    path = Path("src/my_text.txt") # OK
    path.open() # OK, covered by ASYNC230

## Invalid cases:

async def os_path_in_foo():
    file = "file.txt"

    os.path.abspath(file) # ASYNC240
    os.path.exists(file) # ASYNC240

async def pathlib_path_in_foo():
    path = Path("src/my_text.txt")
    path.exists() # ASYNC240

async def pathlib_path_in_foo():
    import pathlib

    path = pathlib.Path("src/my_text.txt")
    path.exists() # ASYNC240

async def inline_path_method_call():
    Path("src/my_text.txt").exists() # ASYNC240
    Path("src/my_text.txt").absolute().exists() # ASYNC240

async def aliased_path_in_foo():
    from pathlib import Path as PathAlias

    path = PathAlias("src/my_text.txt")
    path.exists() # ASYNC240

global_path = Path("src/my_text.txt")

async def global_path_in_foo():
    global_path.exists() # ASYNC240

async def path_as_simple_parameter_type(path: Path):
    path.exists() # ASYNC240

async def path_as_union_parameter_type(path: Path | None):
    path.exists() # ASYNC240

async def path_as_optional_parameter_type(path: Optional[Path]):
    path.exists() # ASYNC240


