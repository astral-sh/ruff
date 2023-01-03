key in obj.keys()  # SIM118

foo["bar"] in obj.keys()  # SIM118

foo['bar'] in obj.keys()  # SIM118

foo() in obj.keys()  # SIM118

for key in obj.keys():  # SIM118
    pass

for key in list(obj.keys()):
    if some_property(key):
        del obj[key]
