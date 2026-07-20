//! Tests for the `ruff config` subcommand.
use std::process::Command;

use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};

const BIN_NAME: &str = "ruff";

#[test]
fn lint_select() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME)).arg("config").arg("lint.select"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    A list of rule codes or prefixes to enable. Prefixes can specify exact
    rules (like `F841`), entire categories (like `F`), or anything in
    between.

    When breaking ties between enabled and disabled rules (via `select` and
    `ignore`, respectively), more specific prefixes override less
    specific prefixes. `ignore` takes precedence over `select` if the
    same prefix appears in both.

    Default value: [
                "ASYNC100", "ASYNC105", "ASYNC115", "ASYNC116", "ASYNC210",
                "ASYNC220", "ASYNC221", "ASYNC222", "ASYNC230", "ASYNC251", "B002",
                "B003", "B004", "B005", "B006", "B008", "B009", "B010", "B012",
                "B013", "B014", "B015", "B016", "B017", "B018", "B019", "B020",
                "B021", "B022", "B023", "B025", "B026", "B029", "B030", "B031",
                "B032", "B033", "B035", "B039", "BLE001", "C400", "C401", "C402",
                "C403", "C404", "C405", "C406", "C408", "C409", "C410", "C411",
                "C413", "C414", "C415", "C417", "C418", "C419", "D419", "DTZ001",
                "DTZ002", "DTZ003", "DTZ004", "DTZ005", "DTZ006", "DTZ007",
                "DTZ011", "DTZ012", "DTZ901", "E722", "E902", "EXE001", "EXE002",
                "EXE004", "EXE005", "F401", "F402", "F404", "F407", "F501", "F502",
                "F503", "F504", "F505", "F506", "F507", "F508", "F509", "F521",
                "F522", "F523", "F524", "F525", "F541", "F601", "F602", "F621",
                "F622", "F631", "F632", "F633", "F634", "F701", "F702", "F704",
                "F706", "F707", "F811", "F821", "F822", "F823", "F841", "F842",
                "F901", "FA100", "FA102", "FLY002", "FURB105", "FURB122", "FURB129",
                "FURB132", "FURB136", "FURB157", "FURB161", "FURB162", "FURB163",
                "FURB166", "FURB167", "FURB168", "FURB169", "FURB177", "FURB181",
                "FURB188", "G010", "G101", "G201", "G202", "I001", "INT001",
                "INT002", "INT003", "LOG001", "LOG002", "LOG009", "LOG014",
                "LOG015", "N999", "PERF101", "PERF102", "PERF402", "PGH005",
                "PIE790", "PIE794", "PIE796", "PIE800", "PIE804", "PIE807",
                "PIE808", "PIE810", "PLC0105", "PLC0131", "PLC0132", "PLC0205",
                "PLC0206", "PLC0208", "PLC0414", "PLC3002", "PLE0100", "PLE0101",
                "PLE0115", "PLE0116", "PLE0117", "PLE0118", "PLE0303", "PLE0305",
                "PLE0307", "PLE0308", "PLE0309", "PLE0604", "PLE0605", "PLE0643",
                "PLE0704", "PLE1132", "PLE1142", "PLE1205", "PLE1206", "PLE1300",
                "PLE1307", "PLE1310", "PLE1507", "PLE1519", "PLE1520", "PLE1700",
                "PLE2502", "PLE2510", "PLE2512", "PLE2513", "PLE2514", "PLE2515",
                "PLR0124", "PLR0133", "PLR0206", "PLR0402", "PLR1704", "PLR1711",
                "PLR1716", "PLR1722", "PLR1730", "PLR1733", "PLR1736", "PLR2044",
                "PLW0120", "PLW0127", "PLW0128", "PLW0129", "PLW0131", "PLW0133",
                "PLW0177", "PLW0211", "PLW0245", "PLW0406", "PLW0602", "PLW0604",
                "PLW0642", "PLW0711", "PLW1501", "PLW1507", "PLW1508", "PLW1509",
                "PLW1510", "PLW2101", "PT010", "PT014", "PT020", "PT025", "PT026",
                "PT031", "PTH124", "PTH210", "PYI001", "PYI002", "PYI003", "PYI004",
                "PYI005", "PYI006", "PYI007", "PYI008", "PYI009", "PYI010",
                "PYI012", "PYI013", "PYI015", "PYI016", "PYI017", "PYI018",
                "PYI019", "PYI020", "PYI025", "PYI026", "PYI029", "PYI030",
                "PYI032", "PYI033", "PYI034", "PYI035", "PYI036", "PYI041",
                "PYI042", "PYI043", "PYI044", "PYI045", "PYI046", "PYI047",
                "PYI048", "PYI049", "PYI050", "PYI052", "PYI055", "PYI057",
                "PYI058", "PYI059", "PYI061", "PYI062", "PYI063", "PYI064",
                "PYI066", "RET501", "RUF007", "RUF008", "RUF009", "RUF010",
                "RUF012", "RUF013", "RUF015", "RUF016", "RUF017", "RUF018",
                "RUF019", "RUF020", "RUF022", "RUF023", "RUF024", "RUF026",
                "RUF028", "RUF030", "RUF032", "RUF033", "RUF034", "RUF040",
                "RUF041", "RUF046", "RUF048", "RUF049", "RUF051", "RUF053",
                "RUF057", "RUF058", "RUF059", "RUF100", "RUF101", "RUF200", "S102",
                "S110", "S112", "SIM101", "SIM102", "SIM103", "SIM107", "SIM113",
                "SIM114", "SIM115", "SIM117", "SIM118", "SIM201", "SIM202",
                "SIM208", "SIM210", "SIM211", "SIM220", "SIM221", "SIM222",
                "SIM223", "SIM401", "SIM905", "SIM911", "T100", "TC004", "TC005",
                "TC007", "TC010", "TRY002", "TRY004", "TRY201", "TRY203", "TRY401",
                "UP001", "UP003", "UP004", "UP005", "UP006", "UP007", "UP008",
                "UP009", "UP010", "UP011", "UP012", "UP014", "UP017", "UP018",
                "UP019", "UP020", "UP021", "UP022", "UP023", "UP024", "UP025",
                "UP026", "UP028", "UP029", "UP030", "UP031", "UP032", "UP033",
                "UP034", "UP035", "UP036", "UP037", "UP039", "UP040", "UP041",
                "UP043", "UP044", "UP045", "UP046", "UP047", "UP049", "UP050",
                "W605", "YTT101", "YTT102", "YTT103", "YTT201", "YTT202", "YTT203",
                "YTT204", "YTT301", "YTT302", "YTT303",
            ]
    Type: list[RuleSelector]
    Example usage:
    ```toml
    # On top of the defaults, enable flake8-bugbear (`B`) and flake8-quotes (`Q`).
    extend-select = ["B", "Q"]
    ```

    ----- stderr -----
    "#
    );
}

#[test]
fn lint_select_json() {
    assert_cmd_snapshot!(
        Command::new(get_cargo_bin(BIN_NAME)).arg("config").arg("lint.select").arg("--output-format").arg("json"), @r##"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "doc": "A list of rule codes or prefixes to enable. Prefixes can specify exact\nrules (like `F841`), entire categories (like `F`), or anything in\nbetween.\n\nWhen breaking ties between enabled and disabled rules (via `select` and\n`ignore`, respectively), more specific prefixes override less\nspecific prefixes. `ignore` takes precedence over `select` if the\nsame prefix appears in both.",
      "default": "[\n            \"ASYNC100\", \"ASYNC105\", \"ASYNC115\", \"ASYNC116\", \"ASYNC210\",\n            \"ASYNC220\", \"ASYNC221\", \"ASYNC222\", \"ASYNC230\", \"ASYNC251\", \"B002\",\n            \"B003\", \"B004\", \"B005\", \"B006\", \"B008\", \"B009\", \"B010\", \"B012\",\n            \"B013\", \"B014\", \"B015\", \"B016\", \"B017\", \"B018\", \"B019\", \"B020\",\n            \"B021\", \"B022\", \"B023\", \"B025\", \"B026\", \"B029\", \"B030\", \"B031\",\n            \"B032\", \"B033\", \"B035\", \"B039\", \"BLE001\", \"C400\", \"C401\", \"C402\",\n            \"C403\", \"C404\", \"C405\", \"C406\", \"C408\", \"C409\", \"C410\", \"C411\",\n            \"C413\", \"C414\", \"C415\", \"C417\", \"C418\", \"C419\", \"D419\", \"DTZ001\",\n            \"DTZ002\", \"DTZ003\", \"DTZ004\", \"DTZ005\", \"DTZ006\", \"DTZ007\",\n            \"DTZ011\", \"DTZ012\", \"DTZ901\", \"E722\", \"E902\", \"EXE001\", \"EXE002\",\n            \"EXE004\", \"EXE005\", \"F401\", \"F402\", \"F404\", \"F407\", \"F501\", \"F502\",\n            \"F503\", \"F504\", \"F505\", \"F506\", \"F507\", \"F508\", \"F509\", \"F521\",\n            \"F522\", \"F523\", \"F524\", \"F525\", \"F541\", \"F601\", \"F602\", \"F621\",\n            \"F622\", \"F631\", \"F632\", \"F633\", \"F634\", \"F701\", \"F702\", \"F704\",\n            \"F706\", \"F707\", \"F811\", \"F821\", \"F822\", \"F823\", \"F841\", \"F842\",\n            \"F901\", \"FA100\", \"FA102\", \"FLY002\", \"FURB105\", \"FURB122\", \"FURB129\",\n            \"FURB132\", \"FURB136\", \"FURB157\", \"FURB161\", \"FURB162\", \"FURB163\",\n            \"FURB166\", \"FURB167\", \"FURB168\", \"FURB169\", \"FURB177\", \"FURB181\",\n            \"FURB188\", \"G010\", \"G101\", \"G201\", \"G202\", \"I001\", \"INT001\",\n            \"INT002\", \"INT003\", \"LOG001\", \"LOG002\", \"LOG009\", \"LOG014\",\n            \"LOG015\", \"N999\", \"PERF101\", \"PERF102\", \"PERF402\", \"PGH005\",\n            \"PIE790\", \"PIE794\", \"PIE796\", \"PIE800\", \"PIE804\", \"PIE807\",\n            \"PIE808\", \"PIE810\", \"PLC0105\", \"PLC0131\", \"PLC0132\", \"PLC0205\",\n            \"PLC0206\", \"PLC0208\", \"PLC0414\", \"PLC3002\", \"PLE0100\", \"PLE0101\",\n            \"PLE0115\", \"PLE0116\", \"PLE0117\", \"PLE0118\", \"PLE0303\", \"PLE0305\",\n            \"PLE0307\", \"PLE0308\", \"PLE0309\", \"PLE0604\", \"PLE0605\", \"PLE0643\",\n            \"PLE0704\", \"PLE1132\", \"PLE1142\", \"PLE1205\", \"PLE1206\", \"PLE1300\",\n            \"PLE1307\", \"PLE1310\", \"PLE1507\", \"PLE1519\", \"PLE1520\", \"PLE1700\",\n            \"PLE2502\", \"PLE2510\", \"PLE2512\", \"PLE2513\", \"PLE2514\", \"PLE2515\",\n            \"PLR0124\", \"PLR0133\", \"PLR0206\", \"PLR0402\", \"PLR1704\", \"PLR1711\",\n            \"PLR1716\", \"PLR1722\", \"PLR1730\", \"PLR1733\", \"PLR1736\", \"PLR2044\",\n            \"PLW0120\", \"PLW0127\", \"PLW0128\", \"PLW0129\", \"PLW0131\", \"PLW0133\",\n            \"PLW0177\", \"PLW0211\", \"PLW0245\", \"PLW0406\", \"PLW0602\", \"PLW0604\",\n            \"PLW0642\", \"PLW0711\", \"PLW1501\", \"PLW1507\", \"PLW1508\", \"PLW1509\",\n            \"PLW1510\", \"PLW2101\", \"PT010\", \"PT014\", \"PT020\", \"PT025\", \"PT026\",\n            \"PT031\", \"PTH124\", \"PTH210\", \"PYI001\", \"PYI002\", \"PYI003\", \"PYI004\",\n            \"PYI005\", \"PYI006\", \"PYI007\", \"PYI008\", \"PYI009\", \"PYI010\",\n            \"PYI012\", \"PYI013\", \"PYI015\", \"PYI016\", \"PYI017\", \"PYI018\",\n            \"PYI019\", \"PYI020\", \"PYI025\", \"PYI026\", \"PYI029\", \"PYI030\",\n            \"PYI032\", \"PYI033\", \"PYI034\", \"PYI035\", \"PYI036\", \"PYI041\",\n            \"PYI042\", \"PYI043\", \"PYI044\", \"PYI045\", \"PYI046\", \"PYI047\",\n            \"PYI048\", \"PYI049\", \"PYI050\", \"PYI052\", \"PYI055\", \"PYI057\",\n            \"PYI058\", \"PYI059\", \"PYI061\", \"PYI062\", \"PYI063\", \"PYI064\",\n            \"PYI066\", \"RET501\", \"RUF007\", \"RUF008\", \"RUF009\", \"RUF010\",\n            \"RUF012\", \"RUF013\", \"RUF015\", \"RUF016\", \"RUF017\", \"RUF018\",\n            \"RUF019\", \"RUF020\", \"RUF022\", \"RUF023\", \"RUF024\", \"RUF026\",\n            \"RUF028\", \"RUF030\", \"RUF032\", \"RUF033\", \"RUF034\", \"RUF040\",\n            \"RUF041\", \"RUF046\", \"RUF048\", \"RUF049\", \"RUF051\", \"RUF053\",\n            \"RUF057\", \"RUF058\", \"RUF059\", \"RUF100\", \"RUF101\", \"RUF200\", \"S102\",\n            \"S110\", \"S112\", \"SIM101\", \"SIM102\", \"SIM103\", \"SIM107\", \"SIM113\",\n            \"SIM114\", \"SIM115\", \"SIM117\", \"SIM118\", \"SIM201\", \"SIM202\",\n            \"SIM208\", \"SIM210\", \"SIM211\", \"SIM220\", \"SIM221\", \"SIM222\",\n            \"SIM223\", \"SIM401\", \"SIM905\", \"SIM911\", \"T100\", \"TC004\", \"TC005\",\n            \"TC007\", \"TC010\", \"TRY002\", \"TRY004\", \"TRY201\", \"TRY203\", \"TRY401\",\n            \"UP001\", \"UP003\", \"UP004\", \"UP005\", \"UP006\", \"UP007\", \"UP008\",\n            \"UP009\", \"UP010\", \"UP011\", \"UP012\", \"UP014\", \"UP017\", \"UP018\",\n            \"UP019\", \"UP020\", \"UP021\", \"UP022\", \"UP023\", \"UP024\", \"UP025\",\n            \"UP026\", \"UP028\", \"UP029\", \"UP030\", \"UP031\", \"UP032\", \"UP033\",\n            \"UP034\", \"UP035\", \"UP036\", \"UP037\", \"UP039\", \"UP040\", \"UP041\",\n            \"UP043\", \"UP044\", \"UP045\", \"UP046\", \"UP047\", \"UP049\", \"UP050\",\n            \"W605\", \"YTT101\", \"YTT102\", \"YTT103\", \"YTT201\", \"YTT202\", \"YTT203\",\n            \"YTT204\", \"YTT301\", \"YTT302\", \"YTT303\",\n        ]",
      "value_type": "list[RuleSelector]",
      "scope": null,
      "example": "# On top of the defaults, enable flake8-bugbear (`B`) and flake8-quotes (`Q`).\nextend-select = [\"B\", \"Q\"]",
      "deprecated": null
    }

    ----- stderr -----
    "##
    );
}
