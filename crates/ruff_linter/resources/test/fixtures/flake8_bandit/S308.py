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
