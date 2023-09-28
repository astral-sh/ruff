import pathlib
from pathlib import Path

# Errors
cwd = Path().resolve()
cwd = pathlib.Path().resolve()

current_directory = Path().resolve()
current_directory = pathlib.Path().resolve()

dir = Path().resolve()
dir = pathlib.Path().resolve()

# OK
cwd = Path.cwd()
cwd = pathlib.Path.cwd()

current_directory = Path.cwd()
current_directory = pathlib.Path.cwd()

dir = Path.cwd()
dir = pathlib.Path.cwd()
