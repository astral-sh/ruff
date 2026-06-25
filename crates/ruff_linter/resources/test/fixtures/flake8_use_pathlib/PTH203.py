import os.path, pathlib
from pathlib import Path
from os.path import getatime

os.path.getatime("filename")
os.path.getatime(b"filename")
os.path.getatime(Path("filename"))


getatime("filename")
getatime(b"filename")
getatime(Path("filename"))


file = __file__

os.path.getatime(file)
os.path.getatime(filename="filename")
os.path.getatime(filename=Path("filename"))

os.path.getatime(  # comment 1
    # comment 2
    "filename"  # comment 3
    # comment 4
    ,  # comment 5
    # comment 6
)  # comment 7

os.path.getatime("file" + "name")

getatime(Path("filename").resolve())

os.path.getatime(pathlib.Path("filename"))

getatime(Path("dir") / "file.txt")
