# These SHOULD have a warning
print("foo %(foo)d bar %(bar)d" % {"foo": "1", "bar": "2"})

print("%d %d" % (1, 1.1))

"foo %e bar %s" % ("1", 2)

"%d" % "1"  # [bad-string-format-type]
"%(key)d" % {"key": "1"}  # [bad-string-format-type]
"%x" % 1.1  # [bad-string-format-type]
"%(key)x" % {"key": 1.1}  # [bad-string-format-type]
"%d" % []  # [bad-string-format-type]
"%(key)d" % {"key": []}  # [bad-string-format-type]

WORD = "abc"
"%d" % WORD  # [bad-string-format-type]
"%d %s" % (WORD, WORD)  # [bad-string-format-type]

# These SHOULD NOT have a warning
VALUES_TO_FORMAT = (1, "2", 3.0)
"%d %s %f" % VALUES_TO_FORMAT
# Warning: Pylint IS able to throw warnings for the one below, but we do not have the power to do this yet
"%d %d %f" % VALUES_TO_FORMAT  # [bad-string-format-type]

"%s" % "1"

"%s %s %s" % ("1", 2, 3.5)

print("%d %d"
      %
(1, 1.1))

"%s" % 1
"%d" % 1
"%f" % 1
"%s" % 1
"%(key)s" % {"key": 1}
"%d" % 1
"%(key)d" % {"key": 1}
"%f" % 1
"%(key)f" % {"key": 1}
"%d" % 1.1
"%(key)d" % {"key": 1.1}
"%s" % []
"%(key)s" % {"key": []}
"%s" % None
"%(key)s" % {"key": None}
