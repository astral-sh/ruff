# ------------------
# less than examples
# ------------------

a = int(input())
b = int(input())
c = int(input())
if a < b and b < c:  # [boolean-chained-comparison]
    pass

a = int(input())
b = int(input())
c = int(input())
if a < b and b <= c:  # [boolean-chained-comparison]
    pass

a = int(input())
b = int(input())
c = int(input())
if a <= b and b < c:  # [boolean-chained-comparison]
    pass

a = int(input())
b = int(input())
c = int(input())
if a <= b and b <= c:  # [boolean-chained-comparison]
    pass

# ---------------------
# greater than examples
# ---------------------

a = int(input())
b = int(input())
c = int(input())
if a > b and b > c:  # [boolean-chained-comparison]
    pass

a = int(input())
b = int(input())
c = int(input())
if a >= b and b > c:  # [boolean-chained-comparison]
    pass

a = int(input())
b = int(input())
c = int(input())
if a > b and b >= c:  # [boolean-chained-comparison]
    pass

a = int(input())
b = int(input())
c = int(input())
if a >= b and b >= c:  # [boolean-chained-comparison]
    pass

# -----------------------
# multiple fixes examples
# -----------------------

a = int(input())
b = int(input())
c = int(input())
d = int(input())
if a < b and b < c and c < d:  # [boolean-chained-comparison]
    pass

a = int(input())
b = int(input())
c = int(input())
d = int(input())
e = int(input())
if a < b and b < c and c < d  and d < e:  # [boolean-chained-comparison]
    pass

# ------------
# bad examples
# ------------

a = int(input())
b = int(input())
c = int(input())
if a < b and b > c: 
    pass

a = int(input())
b = int(input())
c = int(input())
if a < b and b >= c: 
    pass

a = int(input())
b = int(input())
c = int(input())
if a <= b and b > c: 
    pass

a = int(input())
b = int(input())
c = int(input())
if a <= b and b >= c: 
    pass

# TODO: check if this example should be fixable or not
a = int(input())
b = int(input())
c = int(input())
if a > b and b < c: 
    pass