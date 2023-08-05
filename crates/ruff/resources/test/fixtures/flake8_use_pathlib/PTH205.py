import os.path
from pathlib import Path
from os.path import getctime


os.path.getctime("filename")
os.path.getctime(b"filename")
os.path.getctime(Path("filename"))

getctime("filename")
getctime(b"filename")
getctime(Path("filename"))
