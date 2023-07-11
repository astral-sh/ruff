
# Should not emit for valid access with index
var = "abc"[0]
var = f"abc"[0]
var = [1, 2, 3][0]
var = (1, 2, 3)[0]

# Should not emit for valid access with slice
var = "abc"[0:2]
var = f"abc"[0:2]
var = [1, 2, 3][0:2]
var = (1, 2, 3)[0:2]
var = [1, 2, 3][None:2]
var = [1, 2, 3][0:None]
var = [1, 2, 3][:2]
var = [1, 2, 3][0:]

# Should emit for invalid access on strings
var = "abc"["x"]
var = f"abc"["x"]

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

# Cannot emit on invalid access using variable
x = "x"
var = "abc"[x]

# Cannot emit on invalid access using call
def func():
    return 1
var = "abc"[func()]
