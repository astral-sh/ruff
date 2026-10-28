import os
import glob
from glob import glob as search


extensions_dir = "./extensions"

# PTH207
glob.glob(os.path.join(extensions_dir, "ops", "autograd", "*.cpp"))
list(glob.iglob(os.path.join(extensions_dir, "ops", "autograd", "*.cpp")))
search("*.png")

# if `dir_fd` is set, suppress the diagnostic
glob.glob(os.path.join(extensions_dir, "ops", "autograd", "*.cpp"), dir_fd=1)
list(glob.iglob(os.path.join(extensions_dir, "ops", "autograd", "*.cpp"), dir_fd=1))

# Display-only suggestions in preview.
glob.glob("*.py")
glob.iglob(pathname="src/*.py", root_dir=extensions_dir)
glob.glob("**/*.py", recursive=True)
glob.glob("*.py", root_dir=None, dir_fd=None, include_hidden=True)

# Unsupported patterns and arguments still receive a diagnostic, without a fix.
glob.glob("/tmp/*.py")
glob.glob(r"C:\tmp\*.py")
glob.glob("**/*.py")
glob.glob("a**b", recursive=True)
glob.glob("")
glob.glob(pattern)
glob.glob(b"*.py")
glob.glob("*.py", **options)
glob.glob("*.py", recursive=recursive)
glob.glob("*.py", include_hidden=include_hidden)
glob.glob(
    # Keep this explanation.
    "*.py"
)


def existing_import():
    from pathlib import Path as P

    return glob.iglob("*.py", root_dir="src")


def shadowed_import(Path):
    return glob.glob("*.py")
