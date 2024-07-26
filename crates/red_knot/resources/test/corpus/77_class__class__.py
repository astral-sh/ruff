# From Python-3.4.3/Lib/test/test_super.py

class Foo:
    def test_various___class___pathologies(self):
        # See issue #12370

        class X(): #A):
            def f(self):
                return super().f()
            __class__ = 413

        x = X()

        class X:
            x = __class__

            def f():
                __class__

        class X:
            global __class__
            __class__ = 42
            def f():
                __class__

#        class X:
#            nonlocal __class__
#            __class__ = 42
#            def f():
#                __class__
#        self.assertEqual(__class__, 42)
