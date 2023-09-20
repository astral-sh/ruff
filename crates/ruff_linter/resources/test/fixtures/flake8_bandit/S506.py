import json
import yaml
from yaml import CSafeLoader
from yaml import SafeLoader
from yaml import SafeLoader as NewSafeLoader


def test_yaml_load():
    ystr = yaml.dump({"a": 1, "b": 2, "c": 3})
    y = yaml.load(ystr)
    yaml.dump(y)
    try:
        y = yaml.load(ystr, Loader=yaml.CSafeLoader)
    except AttributeError:
        # CSafeLoader only exists if you build yaml with LibYAML
        y = yaml.load(ystr, Loader=yaml.SafeLoader)


def test_json_load():
    # no issue should be found
    j = json.load("{}")


yaml.load("{}", Loader=yaml.Loader)

# no issue should be found
yaml.load("{}", SafeLoader)
yaml.load("{}", yaml.SafeLoader)
yaml.load("{}", CSafeLoader)
yaml.load("{}", yaml.CSafeLoader)
yaml.load("{}", NewSafeLoader)
