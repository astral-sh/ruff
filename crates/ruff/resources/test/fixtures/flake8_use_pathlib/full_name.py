import os
import os.path
import pathlib

p = "/foo"

a = os.path.abspath(p)
aa = os.chmod(p, mode=511)
aaa = os.mkdir(p)
os.makedirs(p)
os.rename(p, q)
os.replace(p, q)
os.rmdir(p)
os.remove(p)
os.unlink(p)
os.getcwd()
b = os.path.exists(p)
bb = os.path.expanduser(p)
bbb = os.path.isdir(p)
bbbb = os.path.isfile(p)
bbbbb = os.path.islink(p)
os.readlink(p)
os.stat(p)
os.path.isabs(p)
os.path.join(p)
os.path.basename(p)
os.path.dirname(p)
os.path.samefile(p)
os.path.splitext(p)
with open(p) as fp:
    fp.read()
open(p).close()
os.getcwdb()
