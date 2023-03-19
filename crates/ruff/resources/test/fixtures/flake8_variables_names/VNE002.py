val = 0  # [VNE002]
foo = 1  # [VNE002]
bar = 3  # [VNE002]
variable = "a"  # [VNE002]
no = ["AA"]  # [VNE002]
T = {12, 3, "JF"}
info = "test" # [VNE002-strict]

ToCountResult = True
flag = False


def a():
    a = "hi"


def test(test=False):
    pass


def test2(foo=False):  # [VNE002]
    pass


try:
    pass
except Exception as handle:  # [VNE002]
    pass
