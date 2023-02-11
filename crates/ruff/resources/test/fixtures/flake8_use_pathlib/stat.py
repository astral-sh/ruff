import os
from pathlib import Path

os.path.getatime("filename")
os.path.getatime(b"filename")
os.path.getatime(Path("filename"))

os.path.getmtime("filename")
os.path.getmtime(b"filename")
os.path.getmtime(Path("filename"))

os.path.getctime("filename")
os.path.getctime(b"filename")
os.path.getctime(Path("filename"))

os.path.getsize("filename")
os.path.getsize(b"filename")
os.path.getsize(Path("filename"))
os.path.getsize(__file__)
