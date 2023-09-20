# Test absolute imports
# Violation cases
import xml.dom.minidom
import xml.dom.minidom as wrong
from xml.dom import minidom as wrong
from xml.dom import minidom
from xml.dom.minidom import parseString as wrong  # Ensure ICN001 throws on function import.
from xml.dom.minidom import parseString
from xml.dom.minidom import parse, parseString
from xml.dom.minidom import parse as ps, parseString as wrong

# No ICN001 violations
import xml.dom.minidom as md
from xml.dom import minidom as md
from xml.dom.minidom import parseString as pstr
from xml.dom.minidom import parse, parseString as pstr
from xml.dom.minidom import parse as ps, parseString as pstr


# Test relative imports
from ..xml.dom import minidom as okay  # Ensure config "xml.dom.minidom" doesn't catch relative imports
