name = "world"
banana = "banana"

# Errors: f-string used with % operator
f"{banana}" % banana  # RUF073
f"hello %s" % "world"  # RUF073
f"hello %s %s" % (1, 2)  # RUF073
f"{name} %s" % "extra"  # RUF073
f"no placeholders" % banana  # RUF073
f"{'nested'} %s" % 42  # RUF073

# OK: regular string with % operator
"hello %s" % "world"
"%s %s" % (1, 2)
"hello %s" % name
b"hello %s" % (name,)

# OK: f-string without % operator
f"hello {name}"
f"{banana}"

# OK: modulo on non-string types
42 % 10
x = 100 % 3
