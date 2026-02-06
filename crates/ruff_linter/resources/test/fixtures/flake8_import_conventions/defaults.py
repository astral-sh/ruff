def not_checked():
    import math


def unconventional():
    import altair
    import matplotlib.pyplot
    import numpy
    import pandas
    import seaborn
    import tkinter
    import networkx


def unconventional_aliases():
    import altair as altr
    import matplotlib.pyplot as plot
    import numpy as nmp
    import pandas as pdas
    import seaborn as sbrn
    import tkinter as tkr
    import networkx as nxy


def conventional_aliases():
    import altair as alt
    import matplotlib.pyplot as plt
    import numpy as np
    import pandas as pd
    import seaborn as sns
    import tkinter as tk
    import networkx as nx


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
