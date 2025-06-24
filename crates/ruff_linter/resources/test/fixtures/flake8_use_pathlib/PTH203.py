import pathlib
import os.path
from pathlib import Path
from os.path import getatime

filename = "filename"
filename1 = __file__
filename2 = Path("filename")


os.path.getatime("filename")
os.path.getatime(b"filename")
os.path.getatime(Path("filename"))
os.path.getatime(__file__)

os.path.getatime(filename)
os.path.getatime(filename1)
os.path.getatime(filename2)

os.path.getatime(filename="filename")
os.path.getatime(filename=b"filename")
os.path.getatime(filename=Path("filename"))
os.path.getatime(filename=__file__)

getatime("filename")
getatime(b"filename")
getatime(Path("filename"))
getatime(__file__)

getatime(filename="filename")
getatime(filename=b"filename")
getatime(filename=Path("filename"))
getatime(filename=__file__)

getatime(filename)
getatime(filename1)
getatime(filename2)


os.path.getatime(
    "filename", # comment
)

os.path.getatime(
    # comment
    "filename"
    ,
    # comment
)

os.path.getatime(
    # comment
    b"filename"
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