import os.path
from pathlib import Path
from os.path import getatime

os.path.getatime("filename")
os.path.getatime(b"filename")
os.path.getatime(Path("filename"))


getatime("filename")
getatime(b"filename")
getatime(Path("filename"))
