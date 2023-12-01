import os.path
from pathlib import Path
from os.path import getsize


os.path.getsize("filename")
os.path.getsize(b"filename")
os.path.getsize(Path("filename"))
os.path.getsize(__file__)

getsize("filename")
getsize(b"filename")
getsize(Path("filename"))
getsize(__file__)
