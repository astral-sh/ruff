import os
import pathlib
from os.path import getmtime
from pathlib import Path

file = __file__

os.path.getmtime(file)
os.path.getmtime("filename")
os.path.getmtime(Path("filename"))

os.path.getmtime(filename="filename")
os.path.getmtime(filename=Path("filename"))

getmtime("filename")
getmtime(Path("filename"))

os.path.getmtime(
    "filename", # comment
)

os.path.getmtime(
    # comment
    "filename"
    ,
    # comment
)

os.path.getmtime( # comment
    Path(__file__)
    # comment
) # comment

getmtime( # comment
    "filename")

getmtime( # comment
    b"filename",
    #comment
)

os.path.getmtime("file" + "name")

getmtime \
 \
 \
        ( # comment
        "filename",
    )

getmtime(Path("filename").resolve())

os.path.getmtime(pathlib.Path("filename"))