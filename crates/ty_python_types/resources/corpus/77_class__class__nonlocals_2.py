class Outer:
    def f(self):
        class Inner:
            nonlocal __class__
            __class__ = 42
            def f():
                 __class__
