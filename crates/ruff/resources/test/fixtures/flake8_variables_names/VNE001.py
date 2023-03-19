x = 0  # [VNE001]
y = 1  # [VNE001]
z = 3  # [VNE001]
i = 4  # [VNE001-strict]
_ = 21
T = 12


def a():
    a = "hi"


def test(test=False):
    pass


def test2(t=False):  # [VNE001]
    pass


try:
    pass
except Exception as e:  # [VNE001]
    pass
