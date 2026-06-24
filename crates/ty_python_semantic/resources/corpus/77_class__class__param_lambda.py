class Outer:
    def x(self):
        def f(__class__):
            lambda: __class__
