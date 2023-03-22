i = 1
while i < 6:  # [while-used]b
    print(i)
    i = i + 1

while i > 100:  # [while-used]
    i += 1
else:
    print("out of loop!")
