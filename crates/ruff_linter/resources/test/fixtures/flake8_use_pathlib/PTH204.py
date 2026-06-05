import os.path
from pathlib import Path
from os.path import getmtime


os.path.getmtime("filename")
os.path.getmtime(b"filename")
os.path.getmtime(Path("filename"))


getmtime("filename")
getmtime(b"filename")
getmtime(Path("filename"))


fd: int = 1
os.path.getmtime(1)
os.path.getmtime(filename=fd)
getmtime(fd)
getmtime(filename=1)


class AttrHolder:
    fd: int = 1


os.path.getmtime(AttrHolder.fd)
getmtime(AttrHolder.fd)
