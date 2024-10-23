# Based on Python-3.4.3/Lib/test/test_scope.py

def test():
    method_and_var = "var"
    class Test:
        def method_and_var(self):
            return "method"
        def test(self):
            return method_and_var
