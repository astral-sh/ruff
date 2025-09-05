import os
from typing import Optional

## Various os.path cases:

def os_path_in_foo():
    file = "file.txt"

    os.path.abspath(file) # OK
    os.path.exists(file) # OK

async def os_path_in_foo():
    file = "file.txt"

    os.path.abspath(file) # ASYNC240
    os.path.exists(file) # ASYNC240

## Various pathlib.Path cases:
from pathlib import Path

def pathlib_path_in_foo():
    path = Path("src/my_text.txt") # OK
    path.absolute() # OK
    path.exists() # OK
    with path.open() as f: # OK
        ...

async def pathlib_path_in_foo():
    path = Path("src/my_text.txt") # ASYNC240
    path.absolute() # ASYNC240
    path.exists() # ASYNC240
    with path.open() as f: # ASYNC240
        ...

async def pathlib_path_in_foo():
    import pathlib

    path = pathlib.Path("src/my_text.txt") # ASYNC240
    path.absolute() # ASYNC240
    path.exists() # ASYNC240

async def aliased_path_in_foo():
    from pathlib import Path as PathAlias

    path = PathAlias("src/my_text.txt") # ASYNC240
    path.absolute() # ASYNC240
    path.exists() # ASYNC240

global_path = Path("src/my_text.txt")

async def global_path_in_foo():
    global_path.absolute() # ASYNC240
    global_path.exists() # ASYNC240

def path_as_simple_parameter_type(path: Path):
    path.absolute() # OK
    path.exists() # OK

async def path_as_simple_parameter_type(path: Path):
    path.absolute() # ASYNC240
    path.exists() # ASYNC240

async def path_as_union_parameter_type(path: Path | None):
    path.absolute() # ASYNC240
    path.exists() # ASYNC240

async def path_as_optional_parameter_type(path: Optional[Path]):
    path.absolute() # ASYNC240
    path.exists() # ASYNC240

## Valid cases using trio/anyio:

async def trio_path_in_foo():
    from trio import Path

    path = Path("src/my_text.txt") # OK
    await path.absolute() # OK
    await path.exists() # OK

async def anyio_path_in_foo():
    from anyio import Path

    path = Path("src/my_text.txt") # OK
    await path.absolute() # OK
    await path.exists() # OK


