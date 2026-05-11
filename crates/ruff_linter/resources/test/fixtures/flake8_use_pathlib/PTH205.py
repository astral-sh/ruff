import os.path
from pathlib import Path
from os.path import getctime


os.path.getctime("filename")
os.path.getctime(b"filename")
os.path.getctime(Path("filename"))

getctime("filename")
getctime(b"filename")
getctime(Path("filename"))

fd = 1


def get_fd() -> int:
    return fd


os.path.getctime(1)
os.path.getctime(fd)
os.path.getctime(get_fd())
os.path.getctime(filename=1)
getctime(1)
getctime(fd)
getctime(get_fd())
getctime(filename=1)
