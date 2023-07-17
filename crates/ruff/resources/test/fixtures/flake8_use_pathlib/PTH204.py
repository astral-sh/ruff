import os.path
from pathlib import Path
from os.path import getmtime


os.path.getmtime("filename")
os.path.getmtime(b"filename")
os.path.getmtime(Path("filename"))


getmtime("filename")
getmtime(b"filename")
getmtime(Path("filename"))
