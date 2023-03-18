val = 0  # [VN002]
foo = 1  # [VN002]
bar = 3  # [VN002]
variable = "a"  # [VN002]
no = ["AA"]  # [VN002]
T = {12, 3, "JF"}

ToCountResult = True
flag = False


def a():
    a = "hi"


def test(test=False):
    pass


def test2(foo=False):  # [VN002]
    pass


try:
    pass
except Exception as handle:  # [VN002]
    pass
