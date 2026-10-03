r = 3.1  # OK

A = 3.14 * r ** 2  # FURB152

C = 6.28 * r  # FURB152

e = 2.71  # FURB152

r = 3.15  # OK

r = 3.141  # FURB152

r = 3.142  # FURB152

r = 3.1415  # FURB152

r = 3.1416  # FURB152

r = 3.141592  # FURB152

r = 3.141593  # FURB152

r = 3.14159265  # FURB152

r = 3.141592653589793238462643383279  # FURB152

r = 3.14159266  # OK

e = 2.7 # OK

e = 2.718  # FURB152

e = 2.7182  # FURB152

e = 2.7183  # FURB152

e = 2.719  # OK

e = 2.71824  # OK

e = 2.71820001  # OK

e = 2.718200000000001  # OK

e = 2.7182000000000001  # FURB152

# Each of these already denotes exactly the same float as the constant it matches,
# so substituting the constant cannot change a result and the fix is safe. Every
# shorter approximation above computes a different value once rewritten, so those
# fixes are offered as unsafe.
r = 3.141592653589793  # FURB152

e = 2.718281828459045  # FURB152

t = 6.283185307179586  # FURB152
