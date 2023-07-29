# pylint: disable=missing-docstring,consider-using-f-string, pointless-statement

## Old style formatting

"%s %z" % ("hello", "world")  # [bad-format-character]

"%s" "%z" % ("hello", "world")  # [bad-format-character]

"""%s %z""" % ("hello", "world")  # [bad-format-character]

"""%s""" """%z""" % ("hello", "world")  # [bad-format-character]

## New style formatting

"{:s} {:y}".format("hello", "world")  # [bad-format-character]

"{:*^30s}".format("centered")

## F-strings

H, W = "hello", "world"
f"{H} {W}"
f"{H:s} {W:z}"  # [bad-format-character]

f"{1:z}"  # [bad-format-character]
