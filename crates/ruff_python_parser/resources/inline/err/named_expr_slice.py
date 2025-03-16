# even after 3.9, an unparenthesized named expression is not allowed in a slice
lst[x:=1:-1]
lst[1:x:=1]
lst[1:3:x:=1]
