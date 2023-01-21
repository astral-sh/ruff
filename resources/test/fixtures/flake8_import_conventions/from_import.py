# Test absolute imports
import xml.dom.minidom as wrong
from xml.dom import minidom as wrong
from xml.dom.minidom import parseString as wrong  # Ensure ICN001 works on function import.

# Test relative imports
from ..xml.dom import minidom as okay  # Ensure config "xml.dom.minidom" doesn't catch relative imports
