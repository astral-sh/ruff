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

    # Calling `gettext.install()` literall monkey-patches `builtins._ = ...`,
    # so even the fully qualified access of `builtins._()` should be considered
    # a possible `gettext` call.
    import builtins
    another = 42
    z = builtins._("{another} translation string")

    # Usually logging strings use `%`-style string interpolation,
    # but `logging` can be configured to use `{}` the same as f-strings,
    # so these should also be ignored.
    # See https://docs.python.org/3/howto/logging-cookbook.html#formatting-styles
    import logging
    logging.info("yet {another} non-f-string")

    # See https://fastapi.tiangolo.com/tutorial/path-params/
    from fastapi import FastAPI
    app = FastAPI()
    item_id = 42

    @app.get("/items/{item_id}")
    async def read_item(item_id):
        return {"item_id": item_id}


# we shouldn't flag either of these, because the bindings are used elsewhere
# in contexts that make it clear that they're not meant to be f-strings
GLOBAL_STRING = "foo {bar}"
LOGGING_TEMPLATE = "{foo} bar"

def uses_global_strings():
    print(GLOBAL_STRING.format(bar="whatever"))

    import logging
    logging.error(LOGGING_TEMPLATE, 42)


def binding_defined_after_string():
    if bool():
        x = "{foo}"
    else:
        x = "{foo}"
    foo = 42
    print(x.format(foo=foo))
