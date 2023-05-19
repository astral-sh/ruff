# Incorrect
(a := 42)
if True:
    (b := 1)
    print(b)

# Correct
if a := 42:
    print("Success")

a = 0
while (a := a + 1) < 10:
    print("Correct")
