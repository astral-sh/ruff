import os
from datetime import datetime
import numpy as np
import pandas as pd
import pytest

import mikeio
from mikeio.eum import EUMType, ItemInfo, EUMUnit
from mikeio.exceptions import OutsideModelDomainError


@pytest.fixture
def ds1():
    nt = 10
    ne = 7

    d1 = np.zeros([nt, ne]) + 0.1
    d2 = np.zeros([nt, ne]) + 0.2
    data = [d1, d2]

    time = pd.date_range(start=datetime(2000, 1, 1), freq="S", periods=nt)
    items = [ItemInfo("Foo"), ItemInfo("Bar")]
    return mikeio.Dataset(
        data=data, time=time, items=items, geometry=mikeio.Grid1D(nx=7, dx=1)
    )


@pytest.fixture
def ds2():
    nt = 10
    ne = 7

    d1 = np.zeros([nt, ne]) + 1.0
    d2 = np.zeros([nt, ne]) + 2.0
    data = [d1, d2]

    time = pd.date_range(start=datetime(2000, 1, 1), freq="S", periods=nt)
    items = [ItemInfo("Foo"), ItemInfo("Bar")]
    return mikeio.Dataset(data, time, items)


@pytest.fixture
def ds3():

    nt = 100
    d1 = np.zeros([nt, 100, 30]) + 1.5
    d2 = np.zeros([nt, 100, 30]) + 2.0
    d3 = np.zeros([nt, 100, 30]) + 3.0

    data = [d1, d2, d3]

    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo(x) for x in ["Foo", "Bar", "Baz"]]
    return mikeio.Dataset(data, time, items)


def test_create_wrong_data_type_error():

    data = ["item 1", "item 2"]

    nt = 2
    time = pd.date_range(start=datetime(2000, 1, 1), freq="S", periods=nt)

    with pytest.raises(TypeError, match="numpy"):
        mikeio.Dataset(data=data, time=time)


def test_get_names():

    nt = 100
    d = np.zeros([nt, 100, 30]) + 1.0
    time = pd.date_range(start=datetime(2000, 1, 1), freq="S", periods=nt)
    data_vars = {"Foo": mikeio.DataArray(data=d, time=time, item=ItemInfo(name="Foo"))}

    ds = mikeio.Dataset(data_vars)

    assert ds["Foo"].name == "Foo"
    assert ds["Foo"].type == EUMType.Undefined
    assert repr(ds["Foo"].unit) == "undefined"
    assert ds.names == ["Foo"]


def test_properties(ds1):
    nt = 10
    ne = 7
    time = pd.date_range(start=datetime(2000, 1, 1), freq="S", periods=nt)

    assert ds1.names == ["Foo", "Bar"]
    assert ds1.n_items == 2

    assert np.all(ds1.time == time)
    assert ds1.n_timesteps == nt
    assert ds1.timestep == 1
    assert ds1.start_time == time[0]
    assert ds1.end_time == time[-1]

    assert ds1.shape == (nt, ne)
    assert ds1.dims == ("time", "x")
    assert ds1.geometry.nx == 7
    assert ds1._zn is None

    # assert not hasattr(ds1, "keys")   # TODO: remove this
    # assert not hasattr(ds1, "values") # TODO: remove this
    assert isinstance(ds1.items[0], ItemInfo)


def test_pop(ds1):
    da = ds1.pop("Foo")
    assert len(ds1) == 1
    assert ds1.names == ["Bar"]
    assert isinstance(da, mikeio.DataArray)
    assert da.name == "Foo"

    ds1["Foo2"] = da  # re-insert
    assert len(ds1) == 2

    da = ds1.pop(-1)
    assert len(ds1) == 1
    assert ds1.names == ["Bar"]
    assert isinstance(da, mikeio.DataArray)
    assert da.name == "Foo2"


def test_popitem(ds1):
    da = ds1.popitem()
    assert len(ds1) == 1
    assert ds1.names == ["Bar"]
    assert isinstance(da, mikeio.DataArray)
    assert da.name == "Foo"


def test_insert(ds1):
    da = ds1[0].copy()
    da.name = "Baz"

    ds1.insert(2, da)
    assert len(ds1) == 3
    assert ds1.names == ["Foo", "Bar", "Baz"]
    assert ds1[-1] == da


def test_insert_wrong_type(ds1):

    with pytest.raises(ValueError):
        ds1["Foo"] = "Bar"


def test_insert_fail(ds1):
    da = ds1[0]
    with pytest.raises(ValueError, match="Cannot add the same object"):
        ds1.insert(2, da)

    vals = ds1[0].values
    da = ds1[0].copy()
    da.values = vals
    with pytest.raises(ValueError, match="refer to the same data"):
        ds1.insert(2, da)


def test_remove(ds1):
    ds1.remove(-1)
    assert len(ds1) == 1
    assert ds1.names == ["Foo"]

    ds1.remove("Foo")
    assert len(ds1) == 0


def test_index_with_attribute():

    nt = 10000
    d = np.zeros([nt, 100, 30]) + 1.0
    time = pd.date_range(start=datetime(2000, 1, 1), freq="S", periods=nt)

    # We cannot create a mikeio.Dataset with multiple references to the same DataArray
    da = mikeio.DataArray(data=d, time=time)
    data = [da, da]
    with pytest.raises(ValueError):
        mikeio.Dataset(data)

    # We cannot create a mikeio.Dataset with multiple references to the same data
    da1 = mikeio.DataArray(item="Foo", data=d, time=time)
    da2 = mikeio.DataArray(item="Bar", data=d, time=time)
    data = [da1, da2]
    with pytest.raises(ValueError):
        mikeio.Dataset(data)

    da1 = mikeio.DataArray(item="Foo", data=d, time=time)
    da2 = mikeio.DataArray(item="Bar", data=d.copy(), time=time)
    data = [da1, da2]
    ds = mikeio.Dataset(data)
    assert ds["Foo"].name == "Foo"
    assert ds.Bar.name == "Bar"

    assert ds["Foo"] is ds.Foo  # This is the same object

    ds["Foo"] = ds.Foo + 2.0
    assert (
        ds["Foo"] is ds.Foo
    )  # This is now modfied, but both methods points to the same object


def test_getitem_time(ds3):
    # time = pd.date_range("2000-1-2", freq="H", periods=100)
    ds_sel = ds3["2000-1-2"]
    assert ds_sel.n_timesteps == 24
    assert ds_sel.is_equidistant

    ds_sel = ds3["2000-1-2":"2000-1-3 00:00"]
    assert ds_sel.n_timesteps == 25
    assert ds_sel.is_equidistant

    time = ["2000-1-2 04:00:00", "2000-1-2 08:00:00", "2000-1-2 12:00:00"]
    ds_sel = ds3[time]
    assert ds_sel.n_timesteps == 3
    assert ds_sel.is_equidistant

    time = [ds3.time[0], ds3.time[1], ds3.time[7], ds3.time[23]]
    ds_sel = ds3[time]
    assert ds_sel.n_timesteps == 4
    assert not ds_sel.is_equidistant

    ds_sel = ds3[ds3.time[:10]]
    assert ds_sel.n_timesteps == 10
    assert ds_sel.is_equidistant


def test_getitem_multi_indexing_attempted(ds3):
    with pytest.raises(TypeError, match="not allow multi-index"):
        ds3[0, 0]
    with pytest.warns(Warning, match="ambiguity"):
        ds3[0, 1]  # indistinguishable from ds3[(0,1)]
    with pytest.raises(TypeError, match="not allow multi-index"):
        ds3[:, 1]
    with pytest.raises(TypeError, match="not allow multi-index"):
        ds3[-1, [0, 1], 1]


def test_select_subset_isel():

    nt = 100
    d1 = np.zeros([nt, 100, 30]) + 1.5
    d2 = np.zeros([nt, 100, 30]) + 2.0

    d1[0, 10, :] = 2.0
    d2[0, 10, :] = 3.0

    time = pd.date_range(start=datetime(2000, 1, 1), freq="S", periods=nt)

    geometry = mikeio.Grid2D(nx=30, ny=100, bbox=[0, 0, 1, 1])

    data = {
        "Foo": mikeio.DataArray(
            data=d1, time=time, geometry=geometry, item=ItemInfo("Foo")
        ),
        "Bar": mikeio.DataArray(
            data=d2, time=time, geometry=geometry, item=ItemInfo("Bar")
        ),
    }

    ds = mikeio.Dataset(data)

    selds = ds.isel(10, axis=1)

    assert len(selds.items) == 2
    assert len(selds.to_numpy()) == 2
    assert selds["Foo"].shape == (100, 30)
    assert selds["Foo"].to_numpy()[0, 0] == 2.0
    assert selds["Bar"].to_numpy()[0, 0] == 3.0

    selds_named_axis = ds.isel(10, axis="y")

    assert len(selds_named_axis.items) == 2
    assert selds_named_axis["Foo"].shape == (100, 30)


def test_select_subset_isel_axis_out_of_range_error(ds2):

    assert len(ds2.shape) == 2
    dss = ds2.isel(idx=0)

    # After subsetting there is only one dimension
    assert len(dss.shape) == 1

    with pytest.raises(IndexError):
        dss.isel(idx=0, axis=1)


def test_isel_named_axis(ds2: mikeio.Dataset):
    dss = ds2.isel(time=0)
    assert len(dss.shape) == 1


def test_select_temporal_subset_by_idx():

    nt = 100
    d1 = np.zeros([nt, 100, 30]) + 1.5
    d2 = np.zeros([nt, 100, 30]) + 2.0

    d1[0, 10, :] = 2.0
    d2[0, 10, :] = 3.0
    data = [d1, d2]

    time = pd.date_range(start=datetime(2000, 1, 1), freq="S", periods=nt)
    items = [ItemInfo("Foo"), ItemInfo("Bar")]
    ds = mikeio.Dataset(data, time, items)

    selds = ds.isel([0, 1, 2], axis=0)

    assert len(selds) == 2
    assert selds["Foo"].shape == (3, 100, 30)


def test_temporal_subset_fancy():

    nt = (24 * 31) + 1
    d1 = np.zeros([nt, 100, 30]) + 1.5
    d2 = np.zeros([nt, 100, 30]) + 2.0
    data = [d1, d2]

    time = pd.date_range("2000-1-1", freq="H", periods=nt)
    items = [ItemInfo("Foo"), ItemInfo("Bar")]
    ds = mikeio.Dataset(data, time, items)

    assert ds.time[0].hour == 0
    assert ds.time[-1].hour == 0

    selds = ds["2000-01-01 00:00":"2000-01-02 00:00"]

    assert len(selds) == 2
    assert selds["Foo"].shape == (25, 100, 30)

    selds = ds[:"2000-01-02 00:00"]
    assert selds["Foo"].shape == (25, 100, 30)

    selds = ds["2000-01-31 00:00":]
    assert selds["Foo"].shape == (25, 100, 30)

    selds = ds["2000-01-30":]
    assert selds["Foo"].shape == (49, 100, 30)


def test_subset_with_datetime():
    nt = (24 * 31) + 1
    d1 = np.zeros([nt, 100, 30]) + 1.5
    d2 = np.zeros([nt, 100, 30]) + 2.0
    data = [d1, d2]

    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo"), ItemInfo("Bar")]
    ds = mikeio.Dataset(data, time, items)

    dssub = ds[datetime(2000, 1, 2)]
    assert dssub.n_timesteps == 1

    dssub = ds[pd.Timestamp(datetime(2000, 1, 2))]
    assert dssub.n_timesteps == 1

    dssub = ds["2000-1-2"]
    assert dssub.n_timesteps == 24


def test_select_item_by_name():
    nt = 100
    d1 = np.zeros([nt, 100, 30]) + 1.5
    d2 = np.zeros([nt, 100, 30]) + 2.0

    d1[0, 10, :] = 2.0
    d2[0, 10, :] = 3.0
    data = [d1, d2]

    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo"), ItemInfo("Bar")]
    ds = mikeio.Dataset(data, time, items)

    foo_data = ds["Foo"]
    assert foo_data.to_numpy()[0, 10, 0] == 2.0


def test_missing_item_error():
    nt = 100

    da1 = mikeio.DataArray(
        data=np.zeros(nt),
        time=pd.date_range("2000-1-2", freq="H", periods=nt),
        item="Foo",
    )

    da2 = mikeio.DataArray(
        data=np.ones(nt),
        time=pd.date_range("2000-1-2", freq="H", periods=nt),
        item="Bar",
    )

    ds = mikeio.Dataset([da1, da2])

    with pytest.raises(KeyError, match="Baz"):
        ds["Baz"]  # there is no Bar item


def test_select_multiple_items_by_name():
    nt = 100
    d1 = np.zeros([nt, 100, 30]) + 1.5
    d2 = np.zeros([nt, 100, 30]) + 2.0
    d3 = np.zeros([nt, 100, 30]) + 3.0

    data = [d1, d2, d3]

    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    # items = [ItemInfo("Foo"), ItemInfo("Bar"), ItemInfo("Baz")]
    items = [ItemInfo(x) for x in ["Foo", "Bar", "Baz"]]
    ds = mikeio.Dataset(data, time, items)

    assert len(ds) == 3  # Length of a dataset is the number of items

    newds = ds[["Baz", "Foo"]]
    assert newds.items[0].name == "Baz"
    assert newds.items[1].name == "Foo"
    assert newds["Foo"].to_numpy()[0, 10, 0] == 1.5

    assert len(newds) == 2


def test_select_multiple_items_by_index(ds3):
    assert len(ds3) == 3  # Length of a dataset is the number of items

    newds = ds3[[2, 0]]
    assert len(newds) == 2
    assert newds.items[0].name == "Baz"
    assert newds.items[1].name == "Foo"
    assert newds["Foo"].to_numpy()[0, 10, 0] == 1.5


def test_select_multiple_items_by_slice(ds3):
    assert len(ds3) == 3  # Length of a dataset is the number of items

    newds = ds3[:2]
    assert len(newds) == 2
    assert newds.items[0].name == "Foo"
    assert newds.items[1].name == "Bar"
    assert newds["Foo"].to_numpy()[0, 10, 0] == 1.5


def test_select_item_by_iteminfo():
    nt = 100
    d1 = np.zeros([nt, 100, 30]) + 1.5
    d2 = np.zeros([nt, 100, 30]) + 2.0

    d1[0, 10, :] = 2.0
    d2[0, 10, :] = 3.0
    data = [d1, d2]

    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo"), ItemInfo("Bar")]
    ds = mikeio.Dataset(data, time, items)

    foo_item = items[0]

    foo_data = ds[foo_item]
    assert foo_data.to_numpy()[0, 10, 0] == 2.0


def test_select_subset_isel_multiple_idxs():

    nt = 100
    d1 = np.zeros([nt, 100, 30]) + 1.5
    d2 = np.zeros([nt, 100, 30]) + 2.0

    data = [d1, d2]

    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo"), ItemInfo("Bar")]
    ds = mikeio.Dataset(data, time, items)

    selds = ds.isel([10, 15], axis=1)

    assert len(selds.items) == 2
    assert len(selds.to_numpy()) == 2
    assert selds["Foo"].shape == (100, 2, 30)


def test_decribe(ds1):
    df = ds1.describe()
    assert df.columns[0] == "Foo"
    assert df.loc["mean"][1] == pytest.approx(0.2)
    assert df.loc["max"][0] == pytest.approx(0.1)


def test_create_undefined():

    nt = 100
    d1 = np.zeros([nt])
    d2 = np.zeros([nt])

    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    data = {
        "Item 1": mikeio.DataArray(
            data=d1, time=time, item=ItemInfo("Item 1")
        ),  # TODO redundant name
        "Item 2": mikeio.DataArray(data=d2, time=time, item=ItemInfo("Item 2")),
    }

    ds = mikeio.Dataset(data)

    assert len(ds.items) == 2
    assert len(ds.to_numpy()) == 2
    assert ds[0].name == "Item 1"
    assert ds[0].type == EUMType.Undefined


def test_create_named_undefined():

    nt = 100
    d1 = np.zeros([nt])
    d2 = np.zeros([nt])

    data = [d1, d2]

    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    ds = mikeio.Dataset(data=data, time=time, items=["Foo", "Bar"])

    assert len(ds.items) == 2
    assert len(ds.to_numpy()) == 2
    assert ds[1].name == "Bar"
    assert ds[1].type == EUMType.Undefined


def test_to_dataframe_single_timestep():

    nt = 1
    d1 = np.zeros([nt])
    d2 = np.zeros([nt])

    data = [d1, d2]

    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo"), ItemInfo("Bar")]
    ds = mikeio.Dataset(data, time, items)
    df = ds.to_dataframe()

    assert "Bar" in df.columns
    # assert isinstance(df.index, pd.DatetimeIndex)


def test_to_dataframe():

    nt = 100
    d1 = np.zeros([nt])
    d2 = np.zeros([nt])

    data = [d1, d2]

    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo"), ItemInfo("Bar")]
    ds = mikeio.Dataset(data, time, items)
    df = ds.to_dataframe()

    assert list(df.columns) == ["Foo", "Bar"]
    assert isinstance(df.index, pd.DatetimeIndex)


def test_multidimensional_to_dataframe_no_supported():

    nt = 100
    d1 = np.zeros([nt, 2])

    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo")]
    ds = mikeio.Dataset([d1], time, items)

    with pytest.raises(ValueError):
        ds.to_dataframe()


def test_get_data():

    data = []
    nt = 100
    d = np.zeros([nt, 100, 30]) + 1.0
    data.append(d)
    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo")]
    ds = mikeio.Dataset(data, time, items)

    assert ds.shape == (100, 100, 30)


def test_interp_time():

    nt = 4
    d = np.zeros([nt, 10, 3])
    d[1] = 2.0
    d[3] = 4.0
    data = [d]
    time = pd.date_range("2000-1-1", freq="D", periods=nt)
    items = [ItemInfo("Foo")]
    ds = mikeio.Dataset(data, time, items)

    assert ds[0].shape == (nt, 10, 3)

    dsi = ds.interp_time(dt=3600)

    assert ds.time[0] == dsi.time[0]
    assert dsi[0].shape == (73, 10, 3)

    dsi2 = ds.interp_time(freq="2H")
    assert dsi2.timestep == 2 * 3600


def test_interp_time_to_other_dataset():

    # Arrange
    ## mikeio.Dataset 1
    nt = 4
    data = [np.zeros([nt, 10, 3])]
    time = pd.date_range("2000-1-1", freq="D", periods=nt)
    items = [ItemInfo("Foo")]
    ds1 = mikeio.Dataset(data, time, items)
    assert ds1.shape == (nt, 10, 3)

    ## mikeio.Dataset 2
    nt = 12
    data = [np.ones([nt, 10, 3])]
    time = pd.date_range("2000-1-1", freq="H", periods=nt)
    items = [ItemInfo("Foo")]
    ds2 = mikeio.Dataset(data, time, items)

    # Act
    ## interp
    dsi = ds1.interp_time(dt=ds2.time)

    # Assert
    assert dsi.time[0] == ds2.time[0]
    assert dsi.time[-1] == ds2.time[-1]
    assert len(dsi.time) == len(ds2.time)
    assert dsi[0].shape[0] == ds2[0].shape[0]

    # Accept dataset as argument
    dsi2 = ds1.interp_time(ds2)
    assert dsi2.time[0] == ds2.time[0]


def test_extrapolate():
    # Arrange
    ## mikeio.Dataset 1
    nt = 2
    data = [np.zeros([nt, 10, 3])]
    time = pd.date_range("2000-1-1", freq="D", periods=nt)
    items = [ItemInfo("Foo")]
    ds1 = mikeio.Dataset(data, time, items)
    assert ds1.shape == (nt, 10, 3)

    ## mikeio.Dataset 2 partly overlapping with mikeio.Dataset 1
    nt = 3
    data = [np.ones([nt, 10, 3])]
    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo")]
    ds2 = mikeio.Dataset(data, time, items)

    # Act
    ## interp
    dsi = ds1.interp_time(dt=ds2.time, fill_value=1.0)

    # Assert
    assert dsi.time[0] == ds2.time[0]
    assert dsi.time[-1] == ds2.time[-1]
    assert len(dsi.time) == len(ds2.time)
    assert dsi[0].values[0] == pytest.approx(0.0)
    assert dsi[0].values[1] == pytest.approx(1.0)  # filled
    assert dsi[0].values[2] == pytest.approx(1.0)  # filled


def test_extrapolate_not_allowed():
    ## mikeio.Dataset 1
    nt = 2
    data = [np.zeros([nt, 10, 3])]
    time = pd.date_range("2000-1-1", freq="D", periods=nt)
    items = [ItemInfo("Foo")]
    ds1 = mikeio.Dataset(data, time, items)
    assert ds1.shape == (nt, 10, 3)

    ## mikeio.Dataset 2 partly overlapping with mikeio.Dataset 1
    nt = 3
    data = [np.ones([nt, 10, 3])]
    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo")]
    ds2 = mikeio.Dataset(data, time, items)

    with pytest.raises(ValueError):
        dsi = ds1.interp_time(dt=ds2.time, fill_value=1.0, extrapolate=False)


def test_get_data_2():

    nt = 100
    data = []
    d = np.zeros([nt, 100, 30]) + 1.0
    data.append(d)
    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo")]
    ds = mikeio.Dataset(data, time, items)

    assert data[0].shape == (100, 100, 30)


def test_get_data_name():

    nt = 100
    data = []
    d = np.zeros([nt, 100, 30]) + 1.0
    data.append(d)
    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo")]
    ds = mikeio.Dataset(data, time, items)

    assert ds["Foo"].shape == (100, 100, 30)


def test_modify_selected_variable():

    nt = 100

    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo")]
    ds = mikeio.Dataset([np.zeros((nt, 10))], time, items)

    assert ds.Foo.to_numpy()[0, 0] == 0.0

    foo = ds.Foo
    foo_mod = foo + 1.0

    ds["Foo"] = foo_mod
    assert ds.Foo.to_numpy()[0, 0] == 1.0


def test_get_bad_name():
    nt = 100
    data = []
    d = np.zeros([100, 100, 30]) + 1.0
    data.append(d)
    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo")]
    ds = mikeio.Dataset(data, time, items)

    with pytest.raises(Exception):
        ds["BAR"]


def test_flipud():

    nt = 2
    d = np.random.random([nt, 100, 30])
    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo")]
    ds = mikeio.Dataset([d], time, items)

    dsud = ds.copy()
    dsud.flipud()

    assert dsud.shape == ds.shape
    assert dsud["Foo"].to_numpy()[0, 0, 0] == ds["Foo"].to_numpy()[0, -1, 0]


def test_aggregation_workflows(tmpdir):
    filename = "tests/testdata/HD2D.dfsu"
    dfs = mikeio.open(filename)

    ds = dfs.read(items=["Surface elevation", "Current speed"])
    ds2 = ds.max(axis=1)

    outfilename = os.path.join(tmpdir.dirname, "max.dfs0")
    ds2.to_dfs(outfilename)
    assert os.path.isfile(outfilename)

    ds3 = ds.min(axis=1)

    outfilename = os.path.join(tmpdir.dirname, "min.dfs0")
    ds3.to_dfs(outfilename)
    assert os.path.isfile(outfilename)


def test_aggregation_dataset_no_time():
    filename = "tests/testdata/HD2D.dfsu"
    dfs = mikeio.open(filename)
    ds = dfs.read(time=-1, items=["Surface elevation", "Current speed"])

    ds2 = ds.max()
    assert ds2["Current speed"].values == pytest.approx(1.6463733)


def test_aggregations():
    filename = "tests/testdata/gebco_sound.dfs2"
    ds = mikeio.read(filename)

    for axis in [0, 1, 2]:
        ds.mean(axis=axis)
        ds.nanmean(axis=axis)
        ds.nanmin(axis=axis)
        ds.nanmax(axis=axis)

    assert ds.mean().shape == (264, 216)
    assert ds.mean(axis="time").shape == (264, 216)
    assert ds.mean(axis="spatial").shape == (1,)
    assert ds.mean(axis="space").shape == (1,)

    with pytest.raises(ValueError, match="space"):
        ds.mean(axis="spaghetti")

    dsm = ds.mean(axis="time")
    assert dsm.geometry is not None


def test_to_dfs_extension_validation(tmpdir):

    outfilename = os.path.join(tmpdir, "not_gonna_happen.dfs2")

    ds = mikeio.read(
        "tests/testdata/HD2D.dfsu", items=["Surface elevation", "Current speed"]
    )
    with pytest.raises(ValueError) as excinfo:
        ds.to_dfs(outfilename)

    assert "dfsu" in str(excinfo.value)


def test_weighted_average(tmpdir):
    filename = "tests/testdata/HD2D.dfsu"
    dfs = mikeio.open(filename)

    ds = dfs.read(items=["Surface elevation", "Current speed"])

    area = dfs.get_element_area()
    ds2 = ds.average(weights=area, axis=1)

    outfilename = os.path.join(tmpdir.dirname, "average.dfs0")
    ds2.to_dfs(outfilename)
    assert os.path.isfile(outfilename)


def test_quantile_axis1(ds1):
    dsq = ds1.quantile(q=0.345, axis=1)
    assert dsq[0].to_numpy()[0] == 0.1
    assert dsq[1].to_numpy()[0] == 0.2

    assert dsq.n_items == ds1.n_items
    assert dsq.n_timesteps == ds1.n_timesteps

    # q as list
    dsq = ds1.quantile(q=[0.25, 0.75], axis=1)
    assert dsq.n_items == 2 * ds1.n_items
    assert "Quantile 0.75, " in dsq.items[1].name
    assert "Quantile 0.25, " in dsq.items[2].name


def test_quantile_axis0(ds1):
    dsq = ds1.quantile(q=0.345)  # axis=0 is default
    assert dsq[0].to_numpy()[0] == 0.1
    assert dsq[1].to_numpy()[0] == 0.2

    assert dsq.n_items == ds1.n_items
    assert dsq.n_timesteps == 1
    assert dsq.shape[-1] == ds1.shape[-1]

    # q as list
    dsq = ds1.quantile(q=[0.25, 0.75], axis=0)
    assert dsq.n_items == 2 * ds1.n_items
    assert dsq[0].to_numpy()[0] == 0.1
    assert dsq[1].to_numpy()[0] == 0.1
    assert dsq[2].to_numpy()[0] == 0.2
    assert dsq[3].to_numpy()[0] == 0.2

    assert "Quantile 0.75, " in dsq.items[1].name
    assert "Quantile 0.25, " in dsq.items[2].name
    assert "Quantile 0.75, " in dsq.items[3].name


def test_nanquantile():
    q = 0.99
    fn = "tests/testdata/random.dfs0"  # has delete value
    ds = mikeio.read(fn)

    dsq1 = ds.quantile(q=q)
    dsq2 = ds.nanquantile(q=q)

    assert np.isnan(dsq1[0].to_numpy())
    assert not np.isnan(dsq2[0].to_numpy())

    qnt = np.quantile(ds[0].to_numpy(), q=q)
    nqnt = np.nanquantile(ds[0].to_numpy(), q=q)

    assert np.isnan(qnt)
    assert dsq2[0].to_numpy() == nqnt


def test_aggregate_across_items():

    ds = mikeio.read("tests/testdata/State_wlbc_north_err.dfs1")

    with pytest.warns(FutureWarning):  # TODO: remove in 1.5.0
        dam = ds.max(axis="items")

    assert isinstance(dam, mikeio.DataArray)
    assert dam.geometry == ds.geometry
    assert dam.dims == ds.dims
    assert dam.type == ds[-1].type

    dsm = ds.mean(axis="items", keepdims=True)

    assert isinstance(dsm, mikeio.Dataset)
    assert dsm.geometry == ds.geometry
    assert dsm.dims == ds.dims


def test_aggregate_selected_items_dfsu_save_to_new_file(tmpdir):

    ds = mikeio.read("tests/testdata/State_Area.dfsu", items="*Level*")

    assert ds.n_items == 5

    with pytest.warns(FutureWarning):  # TODO: remove keepdims in 1.5.0
        dsm = ds.max(
            axis="items", keepdims=True, name="Max Water Level"
        )  # add a nice name
    assert len(dsm) == 1
    assert dsm[0].name == "Max Water Level"
    assert dsm.geometry == ds.geometry
    assert dsm.dims == ds.dims
    assert dsm[0].type == ds[-1].type

    outfilename = os.path.join(tmpdir, "maxwl.dfsu")
    dsm.to_dfs(outfilename)


def test_copy():
    nt = 100
    d1 = np.zeros([nt, 100, 30]) + 1.5
    d2 = np.zeros([nt, 100, 30]) + 2.0

    data = [d1, d2]

    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo"), ItemInfo("Bar")]
    ds = mikeio.Dataset(data, time, items)

    assert len(ds.items) == 2
    assert len(ds.to_numpy()) == 2
    assert ds[0].name == "Foo"

    ds2 = ds.copy()

    ds2[0].name = "New name"

    assert ds2[0].name == "New name"
    assert ds[0].name == "Foo"


def test_dropna():
    nt = 10
    d1 = np.zeros([nt, 100, 30])
    d2 = np.zeros([nt, 100, 30])

    d1[9:] = np.nan
    d2[8:] = np.nan

    data = [d1, d2]

    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo"), ItemInfo("Bar")]
    ds = mikeio.Dataset(data, time, items)

    assert len(ds.items) == 2
    assert len(ds.to_numpy()) == 2

    ds2 = ds.dropna()

    assert ds2.n_timesteps == 8


def test_default_type():

    item = ItemInfo("Foo")
    assert item.type == EUMType.Undefined
    assert repr(item.unit) == "undefined"


def test_int_is_valid_type_info():

    item = ItemInfo("Foo", 100123)
    assert item.type == EUMType.Viscosity

    item = ItemInfo("U", 100002)
    assert item.type == EUMType.Wind_Velocity


def test_int_is_valid_unit_info():

    item = ItemInfo("U", 100002, 2000)
    assert item.type == EUMType.Wind_Velocity
    assert item.unit == EUMUnit.meter_per_sec
    assert repr(item.unit) == "meter per sec"  # TODO replace _per_ with /


def test_default_unit_from_type():

    item = ItemInfo("Foo", EUMType.Water_Level)
    assert item.type == EUMType.Water_Level
    assert item.unit == EUMUnit.meter
    assert repr(item.unit) == "meter"

    item = ItemInfo("Tp", EUMType.Wave_period)
    assert item.type == EUMType.Wave_period
    assert item.unit == EUMUnit.second
    assert repr(item.unit) == "second"

    item = ItemInfo("Temperature", EUMType.Temperature)
    assert item.type == EUMType.Temperature
    assert item.unit == EUMUnit.degree_Celsius
    assert repr(item.unit) == "degree Celsius"


def test_default_name_from_type():

    item = ItemInfo(EUMType.Current_Speed)
    assert item.name == "Current Speed"
    assert item.unit == EUMUnit.meter_per_sec

    item2 = ItemInfo(EUMType.Current_Direction, EUMUnit.degree)
    assert item2.unit == EUMUnit.degree
    item3 = ItemInfo(
        "Current direction (going to)", EUMType.Current_Direction, EUMUnit.degree
    )
    assert item3.type == EUMType.Current_Direction
    assert item3.unit == EUMUnit.degree


def test_iteminfo_string_type_should_fail_with_helpful_message():

    with pytest.raises(ValueError):

        item = ItemInfo("Water level", "Water level")


def test_item_search():

    res = EUMType.search("level")

    assert len(res) > 0
    assert isinstance(res[0], EUMType)


def test_dfsu3d_dataset():

    filename = "tests/testdata/oresund_sigma_z.dfsu"

    dfsu = mikeio.open(filename)

    ds = dfsu.read()

    text = repr(ds)

    assert len(ds) == 2  # Salinity, Temperature

    dsagg = ds.nanmean(axis=0)  # Time averaged

    assert len(dsagg) == 2  # Salinity, Temperature

    assert dsagg[0].shape[0] == 17118

    assert dsagg.time[0] == ds.time[0]  # Time-averaged data index by start time

    ds_elm = dfsu.read(elements=[0])

    assert len(ds_elm) == 2  # Salinity, Temperature

    dss = ds_elm.squeeze()

    assert len(dss) == 2  # Salinity, Temperature


def test_items_data_mismatch():

    nt = 100
    d = np.zeros([nt, 100, 30]) + 1.0
    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo"), ItemInfo("Bar")]  # Two items is not correct!

    with pytest.raises(ValueError):
        mikeio.Dataset([d], time, items)


def test_time_data_mismatch():

    nt = 100
    d = np.zeros([nt, 100, 30]) + 1.0
    time = pd.date_range(
        "2000-1-2", freq="H", periods=nt + 1
    )  # 101 timesteps is not correct!
    items = [ItemInfo("Foo")]

    with pytest.raises(ValueError):
        mikeio.Dataset([d], time, items)


def test_properties_dfs2():
    filename = "tests/testdata/gebco_sound.dfs2"
    ds = mikeio.read(filename)

    assert ds.n_timesteps == 1
    assert ds.n_items == 1
    assert np.all(ds.shape == (1, 264, 216))
    assert ds.n_elements == (264 * 216)
    assert ds.is_equidistant


def test_properties_dfsu():
    filename = "tests/testdata/oresund_vertical_slice.dfsu"
    ds = mikeio.read(filename)

    assert ds.n_timesteps == 3
    assert ds.start_time == datetime(1997, 9, 15, 21, 0, 0)
    assert ds.end_time == datetime(1997, 9, 16, 3, 0, 0)
    assert ds.timestep == (3 * 3600)
    assert ds.n_items == 2
    assert np.all(ds.shape == (3, 441))
    assert ds.n_elements == 441
    assert ds.is_equidistant


def test_create_empty_data():
    ne = 34
    d = mikeio.Dataset.create_empty_data(n_elements=ne)
    assert len(d) == 1
    assert d[0].shape == (1, ne)

    nt = 100
    d = mikeio.Dataset.create_empty_data(n_timesteps=nt, shape=(3, 4, 6))
    assert len(d) == 1
    assert d[0].shape == (nt, 3, 4, 6)

    ni = 4
    d = mikeio.Dataset.create_empty_data(n_items=ni, n_elements=ne)
    assert len(d) == ni
    assert d[-1].shape == (1, ne)

    with pytest.raises(Exception):
        d = mikeio.Dataset.create_empty_data()

    with pytest.raises(Exception):
        d = mikeio.Dataset.create_empty_data(n_elements=None, shape=None)


def test_create_infer_name_from_eum():

    nt = 100
    d = np.random.uniform(size=nt)

    ds = mikeio.Dataset(
        data=[d],
        time=pd.date_range("2000-01-01", freq="H", periods=nt),
        items=[EUMType.Wind_speed],
    )

    assert isinstance(ds.items[0], ItemInfo)
    assert ds.items[0].type == EUMType.Wind_speed
    assert ds.items[0].name == "Wind speed"


def test_add_scalar(ds1):
    ds2 = ds1 + 10.0
    assert np.all(ds2[0].to_numpy() - ds1[0].to_numpy() == 10.0)

    ds3 = 10.0 + ds1
    assert np.all(ds3[0].to_numpy() == ds2[0].to_numpy())
    assert np.all(ds3[1].to_numpy() == ds2[1].to_numpy())


def test_add_inconsistent_dataset(ds1):

    ds2 = ds1[[0]]

    assert len(ds1) != len(ds2)

    with pytest.raises(ValueError):
        ds1 + ds2

    with pytest.raises(ValueError):
        ds1 * ds2


def test_add_bad_value(ds1):

    with pytest.raises(ValueError):
        ds1 + ["one"]


def test_multiple_bad_value(ds1):

    with pytest.raises(ValueError):
        ds1 * ["pi"]


def test_sub_scalar(ds1):
    ds2 = ds1 - 10.0
    assert isinstance(ds2, mikeio.Dataset)
    assert np.all(ds1[0].to_numpy() - ds2[0].to_numpy() == 10.0)

    ds3 = 10.0 - ds1
    assert isinstance(ds3, mikeio.Dataset)
    assert np.all(ds3[0].to_numpy() == 9.9)
    assert np.all(ds3[1].to_numpy() == 9.8)


def test_mul_scalar(ds1):
    ds2 = ds1 * 2.0
    assert np.all(ds2[0].to_numpy() * 0.5 == ds1[0].to_numpy())

    ds3 = 2.0 * ds1
    assert np.all(ds3[0].to_numpy() == ds2[0].to_numpy())
    assert np.all(ds3[1].to_numpy() == ds2[1].to_numpy())


def test_add_dataset(ds1, ds2):
    ds3 = ds1 + ds2
    assert np.all(ds3[0].to_numpy() == 1.1)
    assert np.all(ds3[1].to_numpy() == 2.2)

    ds4 = ds2 + ds1
    assert np.all(ds3[0].to_numpy() == ds4[0].to_numpy())
    assert np.all(ds3[1].to_numpy() == ds4[1].to_numpy())

    ds2b = ds2.copy()
    ds2b[0].item = ItemInfo(EUMType.Wind_Velocity)
    with pytest.raises(ValueError):
        # item type does not match
        ds1 + ds2b

    ds2c = ds2.copy()
    tt = ds2c.time.to_numpy()
    tt[-1] = tt[-1] + np.timedelta64(1, "s")
    ds2c.time = pd.DatetimeIndex(tt)
    with pytest.raises(ValueError):
        # time does not match
        ds1 + ds2c


def test_sub_dataset(ds1, ds2):
    ds3 = ds2 - ds1
    assert np.all(ds3[0].to_numpy() == 0.9)
    assert np.all(ds3[1].to_numpy() == 1.8)


def test_non_equidistant():
    nt = 4
    d = np.random.uniform(size=nt)

    ds = mikeio.Dataset(
        data=[d],
        time=[
            datetime(2000, 1, 1),
            datetime(2001, 1, 1),
            datetime(2002, 1, 1),
            datetime(2003, 1, 1),
        ],
    )

    assert not ds.is_equidistant


def test_concat_dataarray_by_time():
    da1 = mikeio.read("tests/testdata/tide1.dfs1")[0]
    da2 = mikeio.read("tests/testdata/tide2.dfs1")[0]
    da3 = mikeio.DataArray.concat([da1, da2])

    assert da3.start_time == da1.start_time
    assert da3.start_time < da2.start_time
    assert da3.end_time == da2.end_time
    assert da3.end_time > da1.end_time
    assert da3.n_timesteps == 145
    assert da3.is_equidistant


def test_concat_by_time():
    ds1 = mikeio.read("tests/testdata/tide1.dfs1")
    ds2 = mikeio.read("tests/testdata/tide2.dfs1") + 0.5  # add offset
    ds3 = mikeio.Dataset.concat([ds1, ds2])

    assert isinstance(ds3, mikeio.Dataset)
    assert len(ds1) == len(ds2) == len(ds3)
    assert ds3.start_time == ds1.start_time
    assert ds3.start_time < ds2.start_time
    assert ds3.end_time == ds2.end_time
    assert ds3.end_time > ds1.end_time
    assert ds3.n_timesteps == 145
    assert ds3.is_equidistant


def test_concat_by_time_ndim1():
    ds1 = mikeio.read("tests/testdata/tide1.dfs1").isel(x=0)
    ds2 = mikeio.read("tests/testdata/tide2.dfs1").isel(x=0)
    ds3 = mikeio.Dataset.concat([ds1, ds2])

    assert isinstance(ds3, mikeio.Dataset)
    assert len(ds1) == len(ds2) == len(ds3)
    assert ds3.start_time == ds1.start_time
    assert ds3.start_time < ds2.start_time
    assert ds3.end_time == ds2.end_time
    assert ds3.end_time > ds1.end_time
    assert ds3.n_timesteps == 145
    assert ds3.is_equidistant


def test_concat_by_time_inconsistent_shape_not_possible():
    ds1 = mikeio.read("tests/testdata/tide1.dfs1").isel(x=[0, 1])
    ds2 = mikeio.read("tests/testdata/tide2.dfs1").isel(x=[0, 1, 2])
    with pytest.raises(ValueError, match="Shape"):
        mikeio.Dataset.concat([ds1, ds2])


# TODO: implement this
def test_concat_by_time_no_time():
    ds1 = mikeio.read("tests/testdata/tide1.dfs1", time=0)
    ds2 = mikeio.read("tests/testdata/tide2.dfs1", time=1)
    ds3 = mikeio.Dataset.concat([ds1, ds2])

    assert ds3.n_timesteps == 2


def test_concat_by_time_2():
    ds1 = mikeio.read("tests/testdata/tide1.dfs1", time=range(0, 12))
    ds2 = mikeio.read("tests/testdata/tide2.dfs1")
    ds3 = mikeio.Dataset.concat([ds1, ds2])

    assert ds3.n_timesteps == 109
    assert not ds3.is_equidistant

    # create concatd datasets in 8 chunks of 6 hours
    dsall = []
    for j in range(8):
        dsall.append(
            mikeio.read(
                "tests/testdata/tide1.dfs1", time=range(j * 12, 1 + (j + 1) * 12)
            )
        )
    ds4 = mikeio.Dataset.concat(dsall)
    assert len(dsall) == 8
    assert ds4.n_timesteps == 97
    assert ds4.is_equidistant


def test_renamed_dataset_has_updated_attributes(ds1: mikeio.Dataset):
    assert hasattr(ds1, "Foo")
    assert isinstance(ds1.Foo, mikeio.DataArray)
    ds2 = ds1.rename(dict(Foo="Baz"))
    assert not hasattr(ds2, "Foo")
    assert hasattr(ds2, "Baz")
    assert isinstance(ds2.Baz, mikeio.DataArray)

    # inplace version
    ds1.rename(dict(Foo="Baz"), inplace=True)
    assert not hasattr(ds1, "Foo")
    assert hasattr(ds1, "Baz")
    assert isinstance(ds1.Baz, mikeio.DataArray)


def test_merge_by_item():
    ds1 = mikeio.read("tests/testdata/tide1.dfs1")
    ds2 = mikeio.read("tests/testdata/tide1.dfs1")
    old_name = ds2[0].name
    new_name = old_name + " v2"
    # ds2[0].name = ds2[0].name + " v2"
    ds2.rename({old_name: new_name}, inplace=True)
    ds3 = mikeio.Dataset.merge([ds1, ds2])

    assert isinstance(ds3, mikeio.Dataset)
    assert ds3.n_items == 2
    assert ds3[1].name == ds1[0].name + " v2"


def test_merge_by_item_dfsu_3d():
    ds1 = mikeio.read("tests/testdata/oresund_sigma_z.dfsu", items=[0])
    assert ds1.n_items == 1
    ds2 = mikeio.read("tests/testdata/oresund_sigma_z.dfsu", items=[1])
    assert ds2.n_items == 1

    ds3 = mikeio.Dataset.merge([ds1, ds2])

    assert isinstance(ds3, mikeio.Dataset)
    itemnames = [x.name for x in ds3.items]
    assert "Salinity" in itemnames
    assert "Temperature" in itemnames
    assert ds3.n_items == 2


def test_to_numpy(ds2):

    X = ds2.to_numpy()

    assert X.shape == (ds2.n_items,) + ds2.shape
    assert isinstance(X, np.ndarray)


def test_concat():
    filename = "tests/testdata/HD2D.dfsu"
    ds1 = mikeio.read(filename, time=[0, 1])
    ds2 = mikeio.read(filename, time=[2, 3])
    ds3 = mikeio.Dataset.concat([ds1, ds2])
    ds3.n_timesteps

    assert ds1.n_items == ds2.n_items == ds3.n_items
    assert ds3.n_timesteps == (ds1.n_timesteps + ds2.n_timesteps)
    assert ds3.start_time == ds1.start_time
    assert ds3.end_time == ds2.end_time
    assert type(ds3.geometry) == type(ds1.geometry)
    assert ds3.geometry.n_elements == ds1.geometry.n_elements


def test_concat_dfsu3d():
    filename = "tests/testdata/basin_3d.dfsu"
    ds = mikeio.read(filename)
    ds1 = mikeio.read(filename, time=[0, 1])
    ds2 = mikeio.read(filename, time=[1, 2])
    ds3 = mikeio.Dataset.concat([ds1, ds2])

    assert ds1.n_items == ds2.n_items == ds3.n_items
    assert ds3.start_time == ds.start_time
    assert ds3.end_time == ds.end_time
    assert type(ds3.geometry) == type(ds.geometry)
    assert ds3.geometry.n_elements == ds1.geometry.n_elements
    assert ds3._zn.shape == ds._zn.shape
    assert np.all(ds3._zn == ds._zn)


def test_concat_dfsu3d_single_timesteps():
    filename = "tests/testdata/basin_3d.dfsu"
    ds = mikeio.read(filename)
    ds1 = mikeio.read(filename, time=0)
    ds2 = mikeio.read(filename, time=2)
    ds3 = mikeio.Dataset.concat([ds1, ds2])

    assert ds1.n_items == ds2.n_items == ds3.n_items
    assert ds3.start_time == ds1.start_time
    assert ds3.end_time == ds2.end_time


def test_concat_dfs2_single_timesteps():
    filename = "tests/testdata/single_row.dfs2"
    ds = mikeio.read(filename)
    ds1 = mikeio.read(filename, time=0)
    ds2 = mikeio.read(filename, time=2)
    ds3 = mikeio.Dataset.concat([ds1, ds2])

    assert ds1.n_items == ds2.n_items == ds3.n_items
    assert ds3.start_time == ds1.start_time
    assert ds3.end_time == ds2.end_time
    assert ds3.n_timesteps == 2


def test_merge_same_name_error():
    filename = "tests/testdata/HD2D.dfsu"
    ds1 = mikeio.read(filename, items=0)
    ds2 = mikeio.read(filename, items=0)

    assert ds1.items[0].name == ds2.items[0].name

    with pytest.raises(ValueError):
        mikeio.Dataset.merge([ds1, ds2])


def test_incompatible_data_not_allowed():

    da1 = mikeio.read("tests/testdata/HD2D.dfsu")[0]
    da2 = mikeio.read("tests/testdata/oresundHD_run1.dfsu")[1]

    with pytest.raises(ValueError) as excinfo:
        mikeio.Dataset([da1, da2])

    assert "shape" in str(excinfo.value).lower()

    da1 = mikeio.read("tests/testdata/tide1.dfs1")[0]
    da2 = mikeio.read("tests/testdata/tide2.dfs1")[0]

    with pytest.raises(ValueError) as excinfo:
        mikeio.Dataset([da1, da2])

    assert "name" in str(excinfo.value).lower()

    da1 = mikeio.read("tests/testdata/tide1.dfs1")[0]
    da2 = mikeio.read("tests/testdata/tide2.dfs1")[0]
    da2.name = "Foo"

    with pytest.raises(ValueError) as excinfo:
        mikeio.Dataset([da1, da2])

    assert "time" in str(excinfo.value).lower()


def test_xzy_selection():
    # select in space via x,y,z coordinates test
    filename = "tests/testdata/oresund_sigma_z.dfsu"
    ds = mikeio.read(filename)

    with pytest.raises(OutsideModelDomainError):
        ds.sel(x=340000, y=15.75, z=0)


def test_layer_selection():
    # select layer test
    filename = "tests/testdata/oresund_sigma_z.dfsu"
    ds = mikeio.read(filename)

    dss_layer = ds.sel(layers=0)
    # should not be layered after selection
    assert type(dss_layer.geometry) == mikeio.spatial.FM_geometry.GeometryFM


def test_time_selection():
    # select time test
    nt = 100
    data = []
    d = np.random.rand(nt)
    data.append(d)
    time = pd.date_range("2000-1-2", freq="H", periods=nt)
    items = [ItemInfo("Foo")]
    ds = mikeio.Dataset(data, time, items)

    # check for string input
    dss_t = ds.sel(time="2000-01-05")
    # and index based
    dss_tix = ds.sel(time=80)

    assert dss_t.shape == (24,)
    assert len(dss_tix) == 1


def test_create_dataset_with_many_items():
    n_items = 800
    nt = 2
    time = pd.date_range("2000", freq="H", periods=nt)

    das = []

    for i in range(n_items):
        x = np.random.random(nt)
        da = mikeio.DataArray(data=x, time=time, item=mikeio.ItemInfo(f"Item {i+1}"))
        das.append(da)

    ds = mikeio.Dataset(das)

    assert ds.n_items == n_items


def test_create_array_with_defaults_from_dataset():

    filename = "tests/testdata/oresund_sigma_z.dfsu"
    ds: mikeio.Dataset = mikeio.read(filename)

    values = np.zeros(ds.Temperature.shape)

    da = ds.create_data_array(values)

    assert isinstance(da, mikeio.DataArray)
    assert da.geometry == ds.geometry
    assert all(da.time == ds.time)
    assert da.item.type == mikeio.EUMType.Undefined

    da_name = ds.create_data_array(values, "Foo")

    assert isinstance(da, mikeio.DataArray)
    assert da_name.geometry == ds.geometry
    assert da_name.item.type == mikeio.EUMType.Undefined
    assert da_name.name == "Foo"

    da_eum = ds.create_data_array(
        values, item=mikeio.ItemInfo("TS", mikeio.EUMType.Temperature)
    )

    assert isinstance(da_eum, mikeio.DataArray)
    assert da_eum.geometry == ds.geometry
    assert da_eum.item.type == mikeio.EUMType.Temperature


def test_dataset_plot(ds1):
    ax = ds1.isel(x=0).plot()
    assert len(ax.lines) == 2
    ds2 = ds1 + 0.01
    ax = ds2.isel(x=-1).plot(ax=ax)
    assert len(ax.lines) == 4


def test_interp_na():
    time = pd.date_range("2000", periods=5, freq="D")
    da = mikeio.DataArray(
        data=np.array([np.nan, 1.0, np.nan, np.nan, 4.0]),
        time=time,
        item=ItemInfo(name="Foo"),
    )
    da2 = mikeio.DataArray(
        data=np.array([np.nan, np.nan, np.nan, 4.0, 4.0]),
        time=time,
        item=ItemInfo(name="Bar"),
    )

    ds = mikeio.Dataset([da, da2])

    dsi = ds.interp_na()
    assert np.isnan(dsi[0].to_numpy()[0])
    assert dsi.Foo.to_numpy()[2] == pytest.approx(2.0)
    assert np.isnan(dsi.Foo.to_numpy()[0])

    dsi = ds.interp_na(fill_value="extrapolate")
    assert dsi.Foo.to_numpy()[0] == pytest.approx(0.0)
    assert dsi.Bar.to_numpy()[2] == pytest.approx(4.0)


def test_plot_scatter():
    ds = mikeio.read("tests/testdata/oresund_sigma_z.dfsu", time=0)
    ds.plot.scatter(x="Salinity", y="Temperature", title="S-vs-T")
