# error
obj.startswith("foo") or obj.startswith("bar")
# error
obj.endswith("foo") or obj.endswith("bar")
# error
obj.startswith(foo) or obj.startswith(bar)
# error
obj.startswith(foo) or obj.startswith("foo")

# ok
obj.startswith(("foo",  "bar"))
# ok
obj.endswith(("foo",  "bar"))
# ok
obj.startswith("foo") or obj.endswith("bar")
# ok
obj.startswith("foo") or abc.startswith("bar")
