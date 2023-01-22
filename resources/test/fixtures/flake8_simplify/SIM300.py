# Errors
"yoda" == compare  # SIM300
'yoda' == compare  # SIM300
42 == age  # SIM300
"yoda" <= compare  # SIM300
'yoda' < compare  # SIM300
42 > age  # SIM300

# OK
compare == "yoda"
age == 42
x == y
"yoda" == compare == 1
"yoda" == compare == someothervar
"yoda" == "yoda"
