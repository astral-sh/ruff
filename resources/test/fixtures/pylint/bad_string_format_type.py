# These SHOULD have a warning
print("foo %(foo)d bar %(bar)d" % {"foo": "1", "bar": "2"})

# These SHOULD NOT have a warning
