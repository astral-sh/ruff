# Test cases for missing conventions from flake8-import-conventions

# ICN001: plotly.graph_objects should be imported as go
import plotly.graph_objects  # should require alias
import plotly.graph_objects as go  # ok

# ICN001: statsmodels.api should be imported as sm
import statsmodels.api  # should require alias
import statsmodels.api as sm  # ok

# ICN002: geopandas should not be imported as gpd
import geopandas as gpd  # banned
import geopandas  # ok
import geopandas as gdf  # ok

