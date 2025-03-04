# even after 3.10, an unparenthesized walrus is not allowed in a slice
lst[x:=1:-1]
lst[1:x:=1]
lst[1:3:x:=1]
