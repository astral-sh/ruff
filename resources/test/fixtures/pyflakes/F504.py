# Ruff has no way of knowing if the following are F505s
a = "wrong"
"%(a)s %(c)s" % {a: "?", "b": "!"}  # F504 ("b" not used)

hidden = {"a": "!"}
"%(a)s %(c)s" % {"x": 1, **hidden}  # Ok (cannot see through splat)

"%(a)s" % {"a": 1, r"b": "!"}  # F504 ("b" not used)
"%(a)s" % {'a': 1, u"b": "!"}  # F504 ("b" not used)
