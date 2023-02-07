# These SHOULD have a warning
print("foo %(foo)d bar %(bar)d" % {"foo": "1", "bar": "2"})

"foo %e bar %s" % ("1", 2)

"%d" % "1"
"%(key)d" % {"key": "1"}
"%x" % 1.1
"%(key)x" % {"key": 1.1}
"%d" % []
"%d" % ([],)
"%(key)d" % {"key": []}

print("%d" % ("%s" % ("nested",),))

# These should have a warning, but do not right now do to our limitations
WORD = "abc"
"%d" % WORD
"%d %s" % (WORD, WORD)
VALUES_TO_FORMAT = (1, "2", 3.0)
"%d %d %f" % VALUES_TO_FORMAT

# These SHOULD NOT have a warning
"%d %s %f" % VALUES_TO_FORMAT

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
print("%s" % ("%s" % ("nested",),))
print("%s" % ("%d" % (5,),))
"%d %d" % "1"
"%d" "%d" % "1"
