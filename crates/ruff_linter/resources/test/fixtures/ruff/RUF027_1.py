def do_nothing(a):
    return a


def alternative_formatter(src, **kwargs):
    src.format(**kwargs)


def format2(src, *args):
    pass


# These should not cause an RUF027 message
def negative_cases():
    a = 4
    positive = False
    """{a}"""
    "don't format: {a}"
    c = """  {b} """
    d = "bad variable: {invalid}"
    e = "incorrect syntax: {}"
    f = "uses a builtin: {max}"
    json = "{ positive: false }"
    json2 = "{ 'positive': false }"
    json3 = "{ 'positive': 'false' }"
    alternative_formatter("{a}", a=5)
    formatted = "{a}".fmt(a=7)
    partial = "partial sentence"
    a = _("formatting of {partial} in a translation string is bad practice")
    _("formatting of {partial} in a translation string is bad practice")
    print(_("formatting of {partial} in a translation string is bad practice"))
    print(do_nothing("{a}".format(a=3)))
    print(do_nothing(alternative_formatter("{a}", a=5)))
    print(format(do_nothing("{a}"), a=5))
    print("{a}".to_upper())
    print(do_nothing("{a}").format(a="Test"))
    print(do_nothing("{a}").format2(a))
    print(("{a}" "{c}").format(a=1, c=2))
    print("{a}".attribute.chaining.call(a=2))
    print("{a} {c}".format(a))

    from gettext import gettext as foo
    should = 42
    x = foo("This {should} also be understood as a translation string")

    import django.utils.translations
    y = django.utils.translations.gettext("This {should} be understood as a translation string too!")
