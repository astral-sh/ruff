x = 123456789
x = 123456
x = .1
x = 1.
x = 1E+1
x = 1E-1
x = 1.000_000_01
x = 123456789.123456789
x = 123456789.123456789E123456789
x = 123456789E123456789
x = 123456789J
x = 123456789.123456789J
x = 0XB1ACC
x = 0B1011
x = 0O777
x = 0.000000006
x = 10000
x = 133333

# Attribute access
x = 1. .imag
x = 1E+1.imag
x = 1E-1.real
x = 123456789.123456789.hex()
x = 123456789.123456789E123456789 .real
x = 123456789E123456789 .conjugate()
x = 123456789J.real
x = 123456789.123456789J.__add__(0b1011.bit_length())
x = 0XB1ACC.conjugate()
x = 0B1011 .conjugate()
x = 0O777 .real
x = 0.000000006  .hex()
x = -100.0000J

if 10 .real:
    ...

# This is a type error, not a syntax error
y = 100[no]
y = 100(no)