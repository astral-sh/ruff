lazy import foo
lazy    import    foo,    bar as baz

lazy from a import b
lazy from some.really.long.module.name import first_really_long_name, second_really_long_name as renamed_value
lazy from a import (  # comment
    bar,
)

def f():
    lazy import foo.bar
    lazy from another.really.long.module.path import first_name, second_name as alias
