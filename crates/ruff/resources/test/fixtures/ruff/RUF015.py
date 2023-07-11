<<<<<<< HEAD
x = range(10)

# RUF015
list(x)[0]
list(x)[:1]
list(x)[:1:1]
list(x)[:1:2]
tuple(x)[0]
tuple(x)[:1]
tuple(x)[:1:1]
tuple(x)[:1:2]
list(i for i in x)[0]
list(i for i in x)[:1]
list(i for i in x)[:1:1]
list(i for i in x)[:1:2]
[i for i in x][0]
[i for i in x][:1]
[i for i in x][:1:1]
[i for i in x][:1:2]
=======
# Should not emit for valid access with index
var = "abc"[0]
var = f"abc"[0]
var = [1, 2, 3][0]
var = (1, 2, 3)[0]
var = b"abc"[0]

# Should not emit for valid access with slice
var = "abc"[0:2]
var = f"abc"[0:2]
var = b"abc"[0:2]
var = [1, 2, 3][0:2]
var = (1, 2, 3)[0:2]
var = [1, 2, 3][None:2]
var = [1, 2, 3][0:None]
var = [1, 2, 3][:2]
var = [1, 2, 3][0:]
>>>>>>> b7bfd99fe (Add handling for bytes literals)

# OK (not indexing (solely) the first element)
list(x)
list(x)[1]
list(x)[-1]
list(x)[1:]
list(x)[:3:2]
list(x)[::2]
list(x)[::]
[i for i in x]
[i for i in x][1]
[i for i in x][-1]
[i for i in x][1:]
[i for i in x][:3:2]
[i for i in x][::2]
[i for i in x][::]

<<<<<<< HEAD
# OK (doesn't mirror the underlying list)
[i + 1 for i in x][0]
[i for i in x if i > 5][0]
[(i, i + 1) for i in x][0]

# OK (multiple generators)
y = range(10)
[i + j for i in x for j in y][0]
=======
# Should emit for invalid access on bytes
var = b"abc"["x"]

# Should emit for invalid access on lists and tuples
var = [1, 2, 3]["x"]
var = (1, 2, 3)["x"]

# Should emit for invalid access on list comprehensions
var = [x for x in range(10)]["x"]

# Should emit for invalid access using tuple
var = "abc"[1, 2]

# Should emit for invalid access using string
var = [1, 2]["x"]

# Should emit for invalid access using float
var = [1, 2][0.25]

# Should emit for invalid access using dict
var = [1, 2][{"x": "y"}]

# Should emit for invalid access using dict comp
var = [1, 2][{x: "y" for x in range(2)}]

# Should emit for invalid access using list 
var = [1, 2][2, 3]

# Should emit for invalid access using list comp
var = [1, 2][[x for x in range(2)]]

# Should emit on invalid access using set
var = [1, 2][{"x", "y"}]

# Should emit on invalid access using bytes
var = [1, 2][b"x"]

# Should emit for non-integer slice start
var = [1, 2, 3]["x":2]
var = [1, 2, 3][f"x":2]
var = [1, 2, 3][1.2:2]
var = [1, 2, 3][{"x"}:2]

# Should emit for non-integer slice end
var = [1, 2, 3][0:"x"]
var = [1, 2, 3][0:f"x"]
var = [1, 2, 3][0:1.2]

# Should emit for non-integer slice start and end; should emit twice with specific ranges
var = [1, 2, 3]["x":"y"]

# Should emit once for repeated invalid access
var = [1, 2, 3]["x"]["y"]["z"]

# Cannot emit on invalid access using variable in index
x = "x"
var = "abc"[x]

# Cannot emit on invalid access using call
def func():
    return 1
var = "abc"[func()]

# Cannot emit on invalid access using a variable in parent
x = [1, 2, 3]
var = x["y"]

# Cannot emit for invalid access on byte array
var = bytearray(b"abc")["x"]
>>>>>>> b7bfd99fe (Add handling for bytes literals)
