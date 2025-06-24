import pathlib
import os.path
from pathlib import Path
from os.path import getmtime

filename = "filename"
filename1 = __file__
filename2 = Path("filename")


os.path.getmtime("filename")
os.path.getmtime(b"filename")
os.path.getmtime(Path("filename"))
os.path.getmtime(__file__)

os.path.getmtime(filename)
os.path.getmtime(filename1)
os.path.getmtime(filename2)

os.path.getmtime(filename="filename")
os.path.getmtime(filename=b"filename")
os.path.getmtime(filename=Path("filename"))
os.path.getmtime(filename=__file__)

getmtime("filename")
getmtime(b"filename")
getmtime(Path("filename"))
getmtime(__file__)

getmtime(filename="filename")
getmtime(filename=b"filename")
getmtime(filename=Path("filename"))
getmtime(filename=__file__)

getmtime(filename)
getmtime(filename1)
getmtime(filename2)


os.path.getmtime(
    "filename", # comment
)

os.path.getmtime(
    # comment
    "filename"
    ,
    # comment
)

os.path.getmtime(
    # comment
    b"filename"
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