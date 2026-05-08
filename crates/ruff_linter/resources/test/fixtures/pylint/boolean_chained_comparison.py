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
if a < b and b < c and c < d and d < e:  # [boolean-chained-comparison]
    pass

# ------------
# bad examples
# ------------

a = int(input())
b = int(input())
c = int(input())
if a > b or b > c:
    pass

a = int(input())
b = int(input())
c = int(input())
if a > b and b in (1, 2):
    pass

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

a = int(input())
b = int(input())
c = int(input())
if a > b and b < c:
    pass

# fixes will balance parentheses
(a < b) and b < c
a < b and (b < c)
((a < b) and b < c)
(a < b) and (b < c)
(((a < b))) and (b < c)

(a<b) and b<c and ((c<d))

# should error and fix
a<b<c and c<d

# more involved examples (all should error and fix)
a < ( # sneaky comment
	b
  # more comments 
) and b < c

(
    a
    <b
    # hmmm...
    <c
    and ((c<d))
)

a < (b) and (((b)) < c)

# --------------------------
# attribute access examples
# --------------------------

class X:
    value = 1


x = X()
limit = 2
if 0 < x.value and x.value <= limit:  # [boolean-chained-comparison]
    pass

if limit >= x.value and x.value > 0:  # [boolean-chained-comparison]
    pass

# Not handled by the existing repeated-middle-operand simplification.
if x.value > 0 and x.value <= limit:
    pass
