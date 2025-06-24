import pathlib
import os.path
from pathlib import Path
from os.path import getctime

filename = "filename"
filename1 = __file__
filename2 = Path("filename")


os.path.getctime("filename")
os.path.getctime(b"filename")
os.path.getctime(Path("filename"))
os.path.getctime(__file__)

os.path.getctime(filename)
os.path.getctime(filename1)
os.path.getctime(filename2)

os.path.getctime(filename="filename")
os.path.getctime(filename=b"filename")
os.path.getctime(filename=Path("filename"))
os.path.getctime(filename=__file__)

getctime("filename")
getctime(b"filename")
getctime(Path("filename"))
getctime(__file__)

getctime(filename="filename")
getctime(filename=b"filename")
getctime(filename=Path("filename"))
getctime(filename=__file__)

getctime(filename)
getctime(filename1)
getctime(filename2)


os.path.getctime(
    "filename", # comment
)

os.path.getctime(
    # comment
    "filename"
    ,
    # comment
)

os.path.getctime(
    # comment
    b"filename"
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