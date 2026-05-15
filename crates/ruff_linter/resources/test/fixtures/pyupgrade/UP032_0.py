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
# Soundness — should emit unsafe fix because behavior may differ.
# Refer: https://github.com/astral-sh/ruff/issues/15874
###

# Walrus binding referenced only by the dropped argument.
"{1}".format(x := 1, x)

# Unreferenced argument with side effects (a function call) — the converted
# f-string silently drops the call.
"a".format(foo())

# Format-time accessor mutates state read by another argument.
"{[x]} {}".format(d, len(d))

# Multi-field with accessor and a side-effecting argument.
"{0.attr} {1}".format(obj, side_effect())

# Unreferenced argument is a binary/unary op or subscript that may raise.
"1".format(1 / 0)

# `**locals()` splat — keyword fields resolve to surrounding-scope names.
"reading {filename}".format(**locals())

# `**locals()` mixed with explicit keyword arguments.
"{prefix}: {filename}".format(prefix="info", **locals())

# `**vars()` is equivalent to `**locals()`.
"reading {filename}".format(**vars())

# `**vars(target)` — keyword fields resolve to `target.<name>`.
"{foo}:{bar}".format(**vars(x))

# `**vars(target)` with a non-trivial target.
"{foo}".format(**vars(self.obj))

###
# Soundness — should NOT be flagged at all (would change behavior or be invalid).
# Refer: https://github.com/astral-sh/ruff/issues/15874
###

# Field name is `+0`; Python rejects with `KeyError`, the rule used to accept it.
"{+0}".format(0)

# Attribute name with a leading space; `getattr(x, " a")` not `x.a`.
"{. a}".format(x)

# Format-time `StringIndex` whose quote collides with the only available choice.
"{[']}".format(x)

# Unknown conversion specifier; Python raises `ValueError` at runtime.
"{!?}".format(0)

###
# Soundness — must wrap argument in parens, otherwise output is malformed.
###

# Lambda's colon would be misread as the format-spec separator.
"{}".format(lambda: 1)

# Leading `{` would be confused with an escaped brace inside the f-string.
"{}".format({} | {})

###
# Non-errors
###

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

# Regression https://github.com/astral-sh/ruff/issues/21000
# Fix should parenthesize walrus
if __name__ == "__main__":
    number = 0
    string = "{}".format(number := number + 1)
    print(string)

# Unicode escape
"\N{angle}AOB = {angle}°".format(angle=180)

# Raw string with \N{...}
r"\N{angle}AOB = {angle}°".format(angle=180)
