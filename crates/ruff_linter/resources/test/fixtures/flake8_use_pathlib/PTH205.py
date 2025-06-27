import os
import pathlib
from os.path import getctime
from pathlib import Path

file = __file__

os.path.getctime(file)
os.path.getctime("filename")
os.path.getctime(Path("filename"))

os.path.getctime(filename="filename")
os.path.getctime(filename=Path("filename"))

getctime("filename")
getctime(Path("filename"))

os.path.getctime(
    "filename", # comment
)

os.path.getctime(
    # comment
    "filename"
    ,
    # comment
)

os.path.getctime( # comment
    Path(__file__)
    # comment
) # comment

getctime( # comment
    "filename")

getctime( # comment
    b"filename",
    #comment
)

os.path.getctime("file" + "name")

getctime \
 \
 \
        ( # comment
        "filename",
    )

getctime(Path("filename").resolve())

os.path.getctime(pathlib.Path("filename"))