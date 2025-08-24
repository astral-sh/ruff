from airflow.plugins_manager import AirflowPlugin


class AirflowTestPlugin(AirflowPlugin):
    name = "test_plugin"
    # --- Invalid extensions start
    operators = [PluginOperator]
    sensors = [PluginSensorOperator]
    hooks = [PluginHook]
    executors = [PluginExecutor]
    # --- Invalid extensions end
    macros = [plugin_macro]
    flask_blueprints = [bp]
    appbuilder_views = [v_appbuilder_package]
    appbuilder_menu_items = [appbuilder_mitem, appbuilder_mitem_toplevel]
    global_operator_extra_links = [
        AirflowLink(),
        GithubLink(),
    ]
    operator_extra_links = [
        GoogleLink(),
        AirflowLink2(),
        CustomOpLink(),
        CustomBaseIndexOpLink(1),
    ]
    timetables = [CustomCronDataIntervalTimetable]
    listeners = [empty_listener, ClassBasedListener()]
    ti_deps = [CustomTestTriggerRule()]
    priority_weight_strategies = [CustomPriorityWeightStrategy]
