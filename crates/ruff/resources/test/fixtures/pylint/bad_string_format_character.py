# pylint: disable=missing-docstring,consider-using-f-string, pointless-statement

## Old style formatting

"%s %z" % ("hello", "world")  # [bad-format-character]

"%s" "%z" % ("hello", "world")  # [bad-format-character]

"""%s %z""" % ("hello", "world")  # [bad-format-character]

"""%s""" """%z""" % ("hello", "world")  # [bad-format-character]

## New style formatting

"{:s} {:y}".format("hello", "world")  # [bad-format-character]

"{:*^30s}".format("centered") # OK
"{:{s}}".format("hello", s="s")  # OK (nested replacement value not checked)

"{:{s:y}}".format("hello", s="s")  # [bad-format-character] (nested replacement format spec checked)

## f-strings

H, W = "hello", "world"
f"{H} {W}"
f"{H:s} {W:z}"  # [bad-format-character]

f"{1:z}"  # [bad-format-character]

## False negatives

print(("%" "z") % 1)
