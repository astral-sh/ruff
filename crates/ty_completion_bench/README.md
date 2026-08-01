This directory contains a very small command line tool for ad hoc benchmarking
and profiling of ty's completions.

# Example: a new project

This example shows how to run completions in a freshly created project.
This is useful for testing completions in a fairly isolated and
greenfield environment, but with potentially one or more interesting
dependencies.

```console
mkdir completion-ad-hoc-benchmarking
cd completion-ad-hoc-benchmarking
uv init --app
uv add numpy pandas scikit-learn scipy matplotlib
echo 'read_csv' > main.py
```

And then run the benchmark as if requesting completions when the cursor
is positioned after `read_csv` (byte offset `8`):

```console
path/to/ruff/checkout/target/profiling/ty_completion_bench main.py 8 -q --iters 200
```

Has this output:

```text
total elapsed for initial completions request: 156.942768ms
total elapsed: 1.137609166s, time per completion request: 5.688045ms
```

This runs the uncached case once and then repeats the call 200 times (the
cached case). You can then attach your favorite profiling tool such as `cargo flamegraph`, `perf` or `samply` to the invocation.

The `-q/--quiet` flag can be removed to see the actual completions offered.

# Example: an existing project

You can point this tool at any project where `uv sync` works. For example, to run
completions inside the context of the Home Assistant project:

```console
git clone https://github.com/home-assistant/core home-assistant-core
cd home-assistant-core
echo ATTRREMDU > scratch.py
```

And now run completions once (the uncached case) as if the cursor was
positioned immediately after `ATTRREMDU` (byte offset `9`):

```console
path/to/ruff/checkout/target/profiling/ty_completion_bench scratch.py 9
```

Has this output:

```text
total elapsed for initial completions request: 396.41251ms
ATTR_REMAINING_DURATION (module: homeassistant.components.homekit_controller.switch)
ATTR_RESET_VACUUM_SIDE_BRUSH (module: homeassistant.components.xiaomi_miio.button)
homeassistant.components.overkiz.water_heater.atlantic_domestic_hot_water_production_mlb_component (module: homeassistant.components.overkiz.water_heater.atlantic_domestic_hot_water_production_mlb_component)
homeassistant.components.overkiz.water_heater.atlantic_domestic_hot_water_production_v2_io_component (module: homeassistant.components.overkiz.water_heater.atlantic_domestic_hot_water_production_v2_io_component)
homeassistant.components.overkiz.water_heater.domestic_hot_water_production (module: homeassistant.components.overkiz.water_heater.domestic_hot_water_production)
homeassistant.components.radio_browser.media_source (module: homeassistant.components.radio_browser.media_source)
homeassistant.components.recorder.models.state_attributes (module: homeassistant.components.recorder.models.state_attributes)
homeassistant.components.recorder.table_managers.recorder_runs (module: homeassistant.components.recorder.table_managers.recorder_runs)
test_async_get_platforms_loads_loop_if_already_in_sys_modules (module: tests.test_loader)
test_async_handle_source_entity_changes_source_entity_removed_custom_handler (module: tests.helpers.test_helper_integration)
test_attributes_remote_code_number (module: tests.components.mqtt.test_alarm_control_panel)
test_default_address_config_entries_removed_linux (module: tests.components.bluetooth.test_init)
test_formation_strategy_restore_manual_backup_invalid_upload (module: tests.components.zha.test_config_flow)
test_template_with_trigger_templated_auto_off (module: tests.components.template.test_binary_sensor)
NL80211_ATTR_MAX_REMAIN_ON_CHANNEL_DURATION (module: pyric.net.wireless.nl80211_h)
_assert_extract_from_target_command_result (module: tests.components.websocket_api.test_commands)
-----
found 16 completions
```
