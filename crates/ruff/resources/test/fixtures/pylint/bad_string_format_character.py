# pylint: disable=missing-docstring,consider-using-f-string, pointless-statement

## Old style formatting

"%s %z" % ("hello", "world")  # [bad-format-character]

"%s" "%z" % ("hello", "world")  # [bad-format-character]

"""%s %z""" % ("hello", "world")  # [bad-format-character]

"""%s""" """%z""" % ("hello", "world")  # [bad-format-character]

## New style formatting

"{:s} {:y}".format("hello", "world")  # [bad-format-character]

"{:*^30s}".format("centered") # OK
"{:{s}}".format("hello", s="s")  # OK (nested placeholder value not checked)
"{:{s:y}}".format("hello", s="s")  # [bad-format-character] (nested placeholder format spec checked)
"{0:.{prec}g}".format(1.23, prec=15)  # OK (cannot validate after nested placeholder)
"{0:.{foo}{bar}{foobar}y}".format(...)  # OK (cannot validate after nested placeholders)
"{0:.{foo}x{bar}y{foobar}g}".format(...)  # OK (all nested placeholders are consumed without considering in between chars)

## f-strings

H, W = "hello", "world"
f"{H} {W}"
f"{H:s} {W:z}"  # [bad-format-character]

f"{1:z}"  # [bad-format-character]

## False negatives

print(("%" "z") % 1)
