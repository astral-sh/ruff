import glob
import os
import os.path

bytes_path = b"file.py"
bytes_dir = b"directory"
bytes_src = b"src"
bytes_dst = b"dst"

open(bytes_path)
os.chmod(bytes_path, 0o444)
os.mkdir(bytes_dir)
os.makedirs(bytes_dir)
os.rename(bytes_src, bytes_dst)
os.replace(bytes_src, bytes_dst)
os.rmdir(bytes_dir)
os.remove(bytes_path)
os.unlink(bytes_path)
os.readlink(bytes_path)
os.stat(bytes_path)
os.path.exists(bytes_path)
os.path.expanduser(bytes_path)
os.path.isdir(bytes_dir)
os.path.isfile(bytes_path)
os.path.islink(bytes_path)
os.path.isabs(bytes_path)
os.path.join(bytes_dir, bytes_path)
os.path.basename(bytes_path)
os.path.dirname(bytes_path)
os.path.samefile(bytes_src, bytes_dst)
os.path.splitext(bytes_path)
os.listdir(bytes_dir)
os.symlink(bytes_src, bytes_dst)
glob.glob(bytes_path)
glob.iglob(bytes_path)
