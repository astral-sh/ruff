ápple_count = 4444  # [non-ascii-name]
í: int = 42  # [non-ascii-name]
vár, vér = 1, 2  # [non-ascii-name]
á += 1  # [non-ascii-name]


def fúnc(вeta, βeta):  # [non-ascii-name]
    global あ  # [non-ascii-name]


def func1(pós_árg1, pós_arg2, /, *, kwárg1, kwárg2):  # [non-ascii-name]
    ...


def func2(*várgs, **kwárgs):  # [non-ascii-name]
    ...


class Tést:  # [non-ascii-name]
    def methоd(self):  # contains Cyrillic `о` [non-ascii-name]
        self.áé = "hello"  # [non-ascii-name]


# OK
apple_count = 4444
i: int = 42
a, e = 1, 2
a += 1


def func():
    ...


def func1(pos_arg1, pos_arg2, /, *, kwarg1, kwarg2):
    ...


def func2(*vargs, **kwargs):
    ...


class Test:
    def method(self):
        self.ae = "hello"
