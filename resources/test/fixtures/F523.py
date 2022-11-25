# With indexes
"{0}".format(1, 2)  # F523
"{1}".format(1, 2, 3)  # F523
"{1:{0}}".format(1, 2)  # No issues
"{1:{0}}".format(1, 2, 3)  # F523
"{0}{2}".format(1, 2)  # F523, # F524

# With no indexes
"{}".format(1, 2)  # F523
"{}".format(1, 2, 3)  # F523
"{:{}}".format(1, 2)  # No issues
"{:{}}".format(1, 2, 3)  # F523
