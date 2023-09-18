#: E241
a = (1,  2)
#: Okay
b = (1, 20)
#: E242
a = (1,	2)  # tab before 2
#: Okay
b = (1, 20)  # space before 20
#: E241 E241 E241
# issue 135
more_spaces = [a,    b,
               ef,  +h,
               c,   -d]
