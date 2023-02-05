# These SHOULD have a warning
print("foo %(foo)d bar %(bar)d" % {"foo": "1", "bar": "2"})

print("%d %d" % (1, 1.1))

# These SHOULD NOT have a warning
