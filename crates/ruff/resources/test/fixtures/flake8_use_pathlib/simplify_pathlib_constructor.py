from pathlib import Path
from pathlib import Path as pth

# match
_ = Path(".")
_ = pth(".")

# no match
_ = Path()
print(".")
Path("file.txt")
Path(".", "folder")
