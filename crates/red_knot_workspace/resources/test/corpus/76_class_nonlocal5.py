# Based on Python-3.4.3/Lib/test/test_scope.py

def top_method(self):

    def outer():
        class Test:
            def actual_global(self):
                return str("global")
            def str(self):
                return str(self)
