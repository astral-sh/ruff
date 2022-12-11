key in dict.keys() # SIM118

foo["bar"] in dict.keys() # SIM118

foo() in dict.keys() # SIM118

for key in dict.keys(): # SIM118
    pass

for key in list(dict.keys()):
    if some_property(key):
        del dict[key]
