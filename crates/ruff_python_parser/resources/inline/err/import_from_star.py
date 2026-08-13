def f1():
    from module import *
class C:
    from module import *
def f2():
    from ..module import *
def f3():
    from module import *, *
