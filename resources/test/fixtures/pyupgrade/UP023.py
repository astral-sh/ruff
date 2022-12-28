# These two imports have something after cElementTree, so they should be fixed
from xml.etree.cElementTree import XML, Element, SubElement
import xml.etree.cElementTree as ET

# Weird spacing should not cause issues
from   xml.etree.cElementTree    import  XML
import    xml.etree.cElementTree       as      ET

# Multi line imports should also work fine
from xml.etree.cElementTree import (
    XML,
    Element,
    SubElement,
)

# This does not have anything after cElementTree, so it should not be fixed
import xml.etree.cElementTree
