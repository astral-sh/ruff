from pathlib import Path, PurePath
from pathlib import Path as pth

# match
_ = Path(".")
_ = pth(".")
_ = PurePath(".")
_ = Path("")

# no match
_ = Path()
print(".")
Path("file.txt")
Path(".", "folder")
PurePath(".", "folder")
