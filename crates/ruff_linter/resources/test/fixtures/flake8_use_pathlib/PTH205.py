import os.path
from pathlib import Path
from os.path import getctime


os.path.getctime("filename")
os.path.getctime(b"filename")
os.path.getctime(Path("filename"))

getctime("filename")
getctime(b"filename")
getctime(Path("filename"))


fd: int = 1
os.path.getctime(1)
os.path.getctime(filename=fd)
getctime(fd)
getctime(filename=1)


class AttrHolder:
    fd: int = 1


os.path.getctime(AttrHolder.fd)
getctime(AttrHolder.fd)
