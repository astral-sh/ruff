from collections import defaultdict

# Violation cases: RUF026


def func():
    defaultdict(default_factory=None)  # RUF026


def func():
    defaultdict(default_factory=int)  # RUF026


def func():
    defaultdict(default_factory=float)  # RUF026


def func():
    defaultdict(default_factory=dict)  # RUF026


def func():
    defaultdict(default_factory=list)  # RUF026


def func():
    defaultdict(default_factory=tuple)  # RUF026


def func():
    def foo():
        pass

    defaultdict(default_factory=foo)  # RUF026


def func():
    defaultdict(default_factory=lambda: 1)  # RUF026


def func():
    from collections import deque

    defaultdict(default_factory=deque)  # RUF026


def func():
    class MyCallable:
        def __call__(self):
            pass

    defaultdict(default_factory=MyCallable())  # RUF026


def func():
    defaultdict(default_factory=tuple, member=1)  # RUF026


def func():
    defaultdict(member=1, default_factory=tuple)  # RUF026


def func():
    defaultdict(member=1, default_factory=tuple,)  # RUF026


def func():
    defaultdict(
        member=1,
        default_factory=tuple,
    )  # RUF026


def func():
    defaultdict(
        default_factory=tuple,
        member=1,
    )  # RUF026


# Non-violation cases: RUF026


def func():
    defaultdict(default_factory=1)  # OK


def func():
    defaultdict(default_factory="wdefwef")  # OK


def func():
    defaultdict(default_factory=[1, 2, 3])  # OK


def func():
    defaultdict()  # OK


def func():
    defaultdict(int)  # OK


def func():
    defaultdict(list)  # OK


def func():
    defaultdict(dict)  # OK


def func():
    defaultdict(dict, default_factory=list)  # OK


def func():
    def constant_factory(value):
        return lambda: value

    defaultdict(constant_factory("<missing>"))
