from django.utils.safestring import mark_safe


def bad_func():
    inject = "harmful_input"
    mark_safe(inject)
    mark_safe("I will add" + inject + "to my string")
    mark_safe("I will add %s to my string" % inject)
    mark_safe("I will add {} to my string".format(inject))
    mark_safe(f"I will add {inject} to my string")

def good_func():
    mark_safe("I won't inject anything")


@mark_safe
def some_func():
    return '<script>alert("evil!")</script>'


from django.utils.html import mark_safe


def bad_func():
    inject = "harmful_input"
    mark_safe(inject)
    mark_safe("I will add" + inject + "to my string")
    mark_safe("I will add %s to my string" % inject)
    mark_safe("I will add {} to my string".format(inject))
    mark_safe(f"I will add {inject} to my string")

def good_func():
    mark_safe("I won't inject anything")


@mark_safe
def some_func():
    return '<script>alert("evil!")</script>'


# https://github.com/astral-sh/ruff/issues/15522
map(mark_safe, [])
foo = mark_safe
