"""Check for `while` loops.
W0149 should produce four warnings when checking this file.
"""

while True:
    print("Do something")
    break

while False:
    print("Do something")
    break

i = 3
while i > 0:
    print(i)
    i -= 1

while (j := i) > 0:
    print(j)
    i -= 1
