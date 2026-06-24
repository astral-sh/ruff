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
