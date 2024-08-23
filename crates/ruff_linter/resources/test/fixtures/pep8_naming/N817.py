import mod.CaMel as CM
from mod import CamelCase as CC


# OK depending on configured import convention
import xml.etree.ElementTree as ET
from xml.etree import ElementTree as ET

# Always an error (relative import)
from ..xml.eltree import ElementTree as ET
