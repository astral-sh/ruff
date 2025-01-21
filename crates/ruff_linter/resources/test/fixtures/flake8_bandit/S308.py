from django.utils.safestring import mark_safe


def some_func():
    return mark_safe('<script>alert("evil!")</script>')


@mark_safe
def some_func():
    return '<script>alert("evil!")</script>'


from django.utils.html import mark_safe


def some_func():
    return mark_safe('<script>alert("evil!")</script>')


@mark_safe
def some_func():
    return '<script>alert("evil!")</script>'


# https://github.com/astral-sh/ruff/issues/15522
map(mark_safe, [])
foo = mark_safe
