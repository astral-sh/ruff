# Don't fix when loop variable shadows a non-LoopVar outer binding
x = 0
for x in (1, 2, 3):
    s.add(x)
