# pylint: disable=missing-docstring,consider-using-f-string

print("%s %z" % ("hello", "world"))  # [bad-format-character]

print("%s" "%z" % ("hello", "world"))  # [bad-format-character]

print("""%s %z""" % ("hello", "world"))  # [bad-format-character]

print("""%s""" """%z""" % ("hello", "world"))  # [bad-format-character]
