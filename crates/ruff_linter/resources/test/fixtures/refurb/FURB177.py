import pathlib
from pathlib import Path

# Errors
_ = Path().resolve()
_ = pathlib.Path().resolve()

_ = Path("").resolve()
_ = pathlib.Path("").resolve()

_ = Path(".").resolve()
_ = pathlib.Path(".").resolve()

_ = Path("", **kwargs).resolve()
_ = pathlib.Path("", **kwargs).resolve()

_ = Path(".", **kwargs).resolve()
_ = pathlib.Path(".", **kwargs).resolve()

# OK
_ = Path.cwd()
_ = pathlib.Path.cwd()

_ = Path("foo").resolve()
_ = pathlib.Path("foo").resolve()

_ = Path(".", "foo").resolve()
_ = pathlib.Path(".", "foo").resolve()

_ = Path(*args).resolve()
_ = pathlib.Path(*args).resolve()
