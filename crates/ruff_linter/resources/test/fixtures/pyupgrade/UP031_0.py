a, b, x, y = 1, 2, 3, 4

# UP031
print('%s %s' % (a, b))

print('%s%s' % (a, b))

print("trivial" % ())

print("%s" % ("simple",))

print("%s" % ("%s" % ("nested",),))

print("%s%% percent" % (15,))

print("%f" % (15,))

print("%.f" % (15,))

print("%.3f" % (15,))

print("%3f" % (15,))

print("%-5f" % (5,))

print("%9f" % (5,))

print("%#o" % (123,))

print("brace {} %s" % (1,))

print((
    "foo %s "
    "bar %s" % (x, y)
))

print(
  "%s" % (
    "trailing comma",
        )
)

print("foo %s " % (x,))

print("%(k)s" % {"k": "v"})

print("%(k)s" % {
    "k": "v",
    "i": "j"
})

print("%(to_list)s" % {"to_list": []})

print("%(k)s" % {"k": "v", "i": 1, "j": []})

print("%(ab)s" % {"a" "b": 1})

print("%(a)s" % {"a"  :  1})


print(
    "foo %(foo)s "
    "bar %(bar)s" % {"foo": x, "bar": y}
)

bar = {"bar": y}
print(
    "foo %(foo)s "
    "bar %(bar)s" % {"foo": x, **bar}
)

print("%s \N{snowman}" % (a,))

print("%(foo)s \N{snowman}" % {"foo": 1})

print(("foo %s " "bar %s") % (x, y))

# Single-value expressions
print('Hello %s' % "World")
print('Hello %s' % f"World")
print('Hello %s (%s)' % bar)
print('Hello %s (%s)' % bar.baz)
print('Hello %s (%s)' % bar['bop'])
print('Hello %(arg)s' % bar)
print('Hello %(arg)s' % bar.baz)
print('Hello %(arg)s' % bar['bop'])

# Hanging modulos
(
    "foo %s "
    "bar %s"
) % (x, y)

(
    "foo %(foo)s "
    "bar %(bar)s"
) % {"foo": x, "bar": y}

(
    """foo %s"""
    % (x,)
)

(
    """
    foo %s
    """
    % (x,)
)

"%s" % (
    x,  # comment
)


path = "%s-%s-%s.pem" % (
    safe_domain_name(cn), # common name, which should be filename safe because it is IDNA-encoded, but in case of a malformed cert make sure it's ok to use as a filename
    cert.not_valid_after.date().isoformat().replace("-", ""), # expiration date
    hexlify(cert.fingerprint(hashes.SHA256())).decode("ascii")[0:8], # fingerprint prefix
)

# UP031 (no longer false negatives; now offer potentially unsafe fixes)
'Hello %s' % bar

'Hello %s' % bar.baz

'Hello %s' % bar['bop']

# Not a valid type annotation but this test shouldn't result in a panic.
# Refer: https://github.com/astral-sh/ruff/issues/11736
x: "'%s + %s' % (1, 2)"
