###
# Errors
###

"{} {}".format(a, b)

"{1} {0}".format(a, b)

"{0} {1} {0}".format(a, b)

"{x.y}".format(x=z)

"{x} {y} {x}".format(x=a, y=b)

"{.x} {.y}".format(a, b)

"{} {}".format(a.b, c.d)

"{}".format(a())

"{}".format(a.b())

"{}".format(a.b().c())

"hello {}!".format(name)

"{}{b}{}".format(a, c, b=b)

"{}".format(0x0)

"{} {}".format(a, b)

"""{} {}""".format(a, b)

"foo{}".format(1)

r"foo{}".format(1)

x = "{a}".format(a=1)

print("foo {} ".format(x))

"{a[b]}".format(a=a)

"{a.a[b]}".format(a=a)

"{}{{}}{}".format(escaped, y)

"{}".format(a)

'({}={{0!e}})'.format(a)

"{[b]}".format(a)

'{[b]}'.format(a)

"""{[b]}""".format(a)

'''{[b]}'''.format(a)

"{}".format(
    1
)

"123456789 {}".format(
    1111111111111111111111111111111111111111111111111111111111111111111111111,
)

"""
{}
""".format(1)

aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa = """
{}
""".format(
    111111
)

"{a}" "{b}".format(a=1, b=1)

(
    "{a}"
    "{b}"
).format(a=1, b=1)

(
    "{a}"
    ""
    "{b}"
    ""
).format(a=1, b=1)

(
    (
        # comment
        "{a}"
        # comment
        "{b}"
    )
    # comment
    .format(a=1, b=1)
)

(
    "{a}"
    "b"
).format(a=1)


def d(osname, version, release):
    return"{}-{}.{}".format(osname, version, release)


def e():
    yield"{}".format(1)


assert"{}".format(1)


async def c():
    return "{}".format(await 3)


async def c():
    return "{}".format(1 + await 3)


"{}".format(1 * 2)

###
# Non-errors
###

# False-negative: RustPython doesn't parse the `\N{snowman}`.
"\N{snowman} {}".format(a)

"{".format(a)

"}".format(a)

"{} {}".format(*a)

"{x} {x}".format(arg)

"{x.y} {x.z}".format(arg)

b"{} {}".format(a, b)

"{:{}}".format(x, y)

"{}{}".format(a)

"" "{}".format(a["\\"])

"{}".format(a["b"])

r'"\N{snowman} {}".format(a)'

"123456789 {}".format(
    11111111111111111111111111111111111111111111111111111111111111111111111111,
)

"""
{}
{}
{}
""".format(
1,
2,
111111111111111111111111111111111111111111111111111111111111111111111111111111111111111,
)

aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa = """{}
""".format(
    111111
)

"{}".format(
    [
        1,
        2,
        3,
    ]
)

"{a}".format(
    a=[
        1,
        2,
        3,
    ]
)

(
    "{a}"
    "{1 + 2}"
).format(a=1)

"{}".format(**c)

"{}".format(
    1  # comment
)


# The fixed string will exceed the line length, but it's still smaller than the
# existing line length, so it's fine.
"<Customer: {}, {}, {}, {}, {}>".format(self.internal_ids, self.external_ids, self.properties, self.tags, self.others)

# When fixing, trim the trailing empty string.
raise ValueError("Conflicting configuration dicts: {!r} {!r}"
                 "".format(new_dict, d))

# When fixing, trim the trailing empty string.
raise ValueError("Conflicting configuration dicts: {!r} {!r}"
                 .format(new_dict, d))

raise ValueError(
    "Conflicting configuration dicts: {!r} {!r}"
    "".format(new_dict, d)
)

raise ValueError(
    "Conflicting configuration dicts: {!r} {!r}"
    "".format(new_dict, d)

)

# The first string will be converted to an f-string and the curly braces in the second should be converted to be unescaped
(
    "{}"
    "{{}}"
).format(a)

("{}" "{{}}").format(a)


# Both strings will be converted to an f-string and the curly braces in the second should left escaped
(
    "{}"
    "{{{}}}"
).format(a, b)

("{}" "{{{}}}").format(a, b)

# The dictionary should be parenthesized.
"{}".format({0: 1}[0])

# The dictionary should be parenthesized.
"{}".format({0: 1}.bar)

# The dictionary should be parenthesized.
"{}".format({0: 1}())

# The string shouldn't be converted, since it would require repeating the function call.
"{x} {x}".format(x=foo())
"{0} {0}".format(foo())

# The string _should_ be converted, since the function call is repeated in the arguments.
"{0} {1}".format(foo(), foo())

# The call should be removed, but the string itself should remain.
''.format(self.project)

# The call should be removed, but the string itself should remain.
"".format(self.project)

# Not a valid type annotation but this test shouldn't result in a panic.
# Refer: https://github.com/astral-sh/ruff/issues/11736
x: "'{} + {}'.format(x, y)"
