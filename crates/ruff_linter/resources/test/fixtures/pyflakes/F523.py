# With indexes
"{0}".format(1, 2)  # F523
"{1}".format(1, 2, 3)  # F523
"{1:{0}}".format(1, 2)  # No issues
"{1:{0}}".format(1, 2, 3)  # F523
"{0}{2}".format(1, 2)  # F523, # F524
"{1.arg[1]!r:0{2['arg']}{1}}".format(1, 2, 3, 4) # F523

# With no indexes
"{}".format(1, 2)  # F523
"{}".format(1, 2, 3)  # F523
"{:{}}".format(1, 2)  # No issues
"{:{}}".format(1, 2, 3)  # F523

# With *args
"{0}{1}".format(*args)  # No issues
"{0}{1}".format(1, *args)  # No issues
"{0}{1}".format(1, 2, *args)  # No issues
"{0}{1}".format(1, 2, 3, *args)  # F523

# With nested quotes
"''1{0}".format(1, 2, 3)  # F523
"\"\"{1}{0}".format(1, 2, 3)  # F523
'""{1}{0}'.format(1, 2, 3)  # F523

# With modified indexes
"{1}{2}".format(1, 2, 3)  # F523, # F524
"{1}{3}".format(1, 2, 3, 4)  # F523, # F524
"{1} {8}".format(0, 1)  # F523, # F524

# Multiline
(''
.format(2))
