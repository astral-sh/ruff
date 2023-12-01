def test():
 # fmt: off
 	a_very_small_indent
 	(
not_fixed
 )

 	if True:
# Fun tab, space, tab, space. Followed by space, tab, tab, space
	 	 pass
 		 more
 	else:
  	   other
 # fmt: on

