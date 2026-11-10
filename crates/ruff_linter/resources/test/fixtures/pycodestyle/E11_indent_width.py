#: E111 with indent-width=4 or 8, ok with indent-width=2
if x > 2:
  print(x)
#: E111 with any indent-width (3 is not a multiple of 2, 4, or 8)
if x > 2:
   print(x)
#: E111 and E114 with indent-width=4 or 8, ok with indent-width=2
if True:
  # comment
  print(x)
#: E111 with indent-width=8, ok with indent-width=2 or 4
if x > 2:
    print(x)
