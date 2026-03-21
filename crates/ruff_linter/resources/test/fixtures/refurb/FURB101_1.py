
from pathlib import Path

with Path("file.txt").open() as f:
    contents = f.read()

with Path("file.txt").open("r") as f:
    contents = f.read()