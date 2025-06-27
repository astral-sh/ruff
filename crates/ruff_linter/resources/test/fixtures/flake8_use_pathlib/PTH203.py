import os
import pathlib
from os.path import getatime
from pathlib import Path

file = __file__

os.path.getatime(file)
os.path.getatime("filename")
os.path.getatime(Path("filename"))

os.path.getatime(filename="filename")
os.path.getatime(filename=Path("filename"))

getatime("filename")
getatime(Path("filename"))

os.path.getatime(
    "filename", # comment
)

os.path.getatime(
    # comment
    "filename"
    ,
    # comment
)

os.path.getatime( # comment
    Path(__file__)
    # comment
) # comment

getatime( # comment
    "filename")

getatime( # comment
    b"filename",
    #comment
)

os.path.getatime("file" + "name")

getatime \
 \
 \
        ( # comment
        "filename",
    )

getatime(Path("filename").resolve())

os.path.getatime(pathlib.Path("filename"))