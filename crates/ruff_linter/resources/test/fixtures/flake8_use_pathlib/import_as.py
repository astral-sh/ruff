import os as foo
import os.path as foo_p

p = "/foo"
q = "bar"

a = foo_p.abspath(p)
aa = foo.chmod(p)
aaa = foo.mkdir(p)
foo.makedirs(p)
foo.rename(p)
foo.replace(p)
foo.rmdir(p)
foo.remove(p)
foo.unlink(p)
foo.getcwd(p)
b = foo_p.exists(p)
bb = foo_p.expanduser(p)
bbb = foo_p.isdir(p)
bbbb = foo_p.isfile(p)
bbbbb = foo_p.islink(p)
foo.readlink(p)
foo.stat(p)
foo_p.isabs(p)
foo_p.join(p, q)
foo.sep.join([p, q])
foo.sep.join((p, q))
foo_p.basename(p)
foo_p.dirname(p)
foo_p.samefile(p)
foo_p.splitext(p)

fd = 1


def get_fd() -> int:
    return fd


foo_p.exists(1)
foo_p.exists(fd)
foo_p.exists(get_fd())
foo_p.exists(path=1)
foo_p.isdir(1)
foo_p.isdir(fd)
foo_p.isdir(get_fd())
foo_p.isdir(s=1)
foo_p.isfile(1)
foo_p.isfile(fd)
foo_p.isfile(get_fd())
foo_p.isfile(path=1)
foo_p.samefile(1, p)
foo_p.samefile(fd, p)
foo_p.samefile(get_fd(), p)
foo_p.samefile(f1=1, f2=p)
