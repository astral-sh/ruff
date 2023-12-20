# Errors
"yoda" == compare  # SIM300
42 == age  # SIM300
("a", "b") == compare  # SIM300
"yoda" <= compare  # SIM300
"yoda" < compare  # SIM300
42 > age  # SIM300
-42 > age  # SIM300
+42 > age  # SIM300
YODA == age  # SIM300
YODA > age  # SIM300
YODA >= age  # SIM300
JediOrder.YODA == age  # SIM300
0 < (number - 100)  # SIM300
B<A[0][0]or B
B or(B)<A[0][0]

# Errors in preview
['upper'] == UPPER_LIST
{} == DummyHandler.CONFIG

# Errors in stable
UPPER_LIST == ['upper']
DummyHandler.CONFIG == {}

# OK
compare == "yoda"
age == 42
compare == ("a", "b")
x == y
"yoda" == compare == 1
"yoda" == compare == someothervar
"yoda" == "yoda"
age == YODA
age < YODA
age <= YODA
YODA == YODA
age == JediOrder.YODA
(number - 100) > 0
SECONDS_IN_DAY == 60 * 60 * 24 # Error in 0.1.8
SomeClass().settings.SOME_CONSTANT_VALUE > (60 * 60) # Error in 0.1.8
{"non-empty-dict": "is-ok"} == DummyHandler.CONFIG
