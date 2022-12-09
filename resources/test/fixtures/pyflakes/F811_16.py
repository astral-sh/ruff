"""Test that shadowing a global with a nested function generates a warning."""

import fu


def bar():
    def baz():
        def fu():
            pass
