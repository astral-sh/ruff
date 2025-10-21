def foo(): return "mock"  # fmt: skip
if True: print("yay")  # fmt: skip
for i in range(10): print(i)  # fmt: skip

j =     1 # fmt: skip
while j < 10: j += 1  # fmt: skip

b = [c for c in "A very long string that would normally generate some kind of collapse, since it is this long"] # fmt: skip
