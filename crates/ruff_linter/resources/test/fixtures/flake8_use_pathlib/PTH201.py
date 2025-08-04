from pathlib import Path, PurePath, PosixPath, PurePosixPath, WindowsPath, PureWindowsPath
from pathlib import Path as pth


# match
_ = Path(".")
_ = pth(".")
_ = PurePath(".")
_ = Path("")

Path('', )

Path(
    '',
)

Path(  # Comment before argument
    '',
)

Path(
    '',  # EOL comment
)

Path(
    ''  # Comment in the middle of implicitly concatenated string
    ".",
)

Path(
    ''  # Comment before comma
    ,
)

Path(
    '',
) / "bare"

Path(  # Comment before argument
    '',
) / ("parenthesized")

Path(
    '',  # EOL comment
) / ( ("double parenthesized"  )   )

(  Path(
    ''  # Comment in the middle of implicitly concatenated string
    ".",
) )/ (("parenthesized path call")
      # Comment between closing parentheses
)

Path(
    ''  # Comment before comma
    ,
) / "multiple" / (
    "frag"  # Comment
    'ment'
)


# no match
_ = Path()
print(".")
Path("file.txt")
Path(".", "folder")
PurePath(".", "folder")

Path()

from importlib.metadata import PackagePath

_ = PosixPath(".")
_ = PurePosixPath(".")
_ = WindowsPath(".")
_ = PureWindowsPath(".")
_ = PackagePath(".")
