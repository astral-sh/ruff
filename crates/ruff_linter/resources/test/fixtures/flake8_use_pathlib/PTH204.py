import os.path
from pathlib import Path
from os.path import getmtime


os.path.getmtime("filename")
os.path.getmtime(b"filename")
os.path.getmtime(Path("filename"))


getmtime("filename")
getmtime(b"filename")
getmtime(Path("filename"))

fd = 1


def get_fd() -> int:
    return fd


os.path.getmtime(1)
os.path.getmtime(fd)
os.path.getmtime(get_fd())
os.path.getmtime(filename=1)
getmtime(1)
getmtime(fd)
getmtime(get_fd())
getmtime(filename=1)
