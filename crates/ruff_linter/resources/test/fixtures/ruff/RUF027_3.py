foo = 0


## Errors

print("{foo()}")
print("{foo(non_existent)}")
print("{foo.baz}")
print("{foo['bar']}")

print("{foo().qux}")
print("{foo[lorem].ipsum()}")
print("{foo.dolor[sit]().amet}")


## No errors

print("{foo if consectetur else adipiscing}")
print("{[foo]}")
print("{ {foo} }")
