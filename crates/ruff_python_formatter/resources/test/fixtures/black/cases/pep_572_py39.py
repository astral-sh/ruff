# Unparenthesized walruses are now allowed in set literals & set comprehensions
# since Python 3.9
{x := 1, 2, 3}
{x4 := x**5 for x in range(7)}
# We better not remove the parentheses here (since it's a 3.10 feature)
x[(a := 1)]
x[(a := 1), (b := 3)]
