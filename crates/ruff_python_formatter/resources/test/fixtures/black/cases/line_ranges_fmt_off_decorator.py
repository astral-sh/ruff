# flags: --line-ranges=12-12 --line-ranges=21-21
# NOTE: If you need to modify this file, pay special attention to the --line-ranges=
# flag above as it's formatting specifically these lines.

# Regression test for an edge case involving decorators and fmt: off/on.
class MyClass:

    # fmt: off
    @decorator  (  )
    # fmt: on
    def method():
        print   ( "str" )

    @decor(
        a=1,
        # fmt: off
        b=(2,   3),
        # fmt: on
    )
    def func():
          pass
