"""Test that shadowing a global name with a nested function generates a warning."""
import fu


def bar():
    import fu

    def baz():
        def fu():
            pass
