import { OptionGroup } from "./ruff_options";

export type Config = { [key: string]: { [key: string]: string } };

export function getDefaultConfig(availableOptions: OptionGroup[]): Config {
  const config: Config = {};
  availableOptions.forEach((group) => {
    config[group.name] = {};
    group.fields.forEach((f) => {
      config[group.name][f.name] = f.default;
    });
  });
  return config;
}

/**
 * Convert the config in the application to something Ruff accepts.
 *
 * Application config is always nested one level. Ruff allows for some
 * top-level options.
 *
 * Any option value is parsed as JSON to convert it to a native JS object.
 * If that fails, e.g. while a user is typing, we let the application handle that
 * and show an error.
 */
export function toRuffConfig(config: Config): any {
  const convertValue = (value: string): any => {
    return value === "None" ? null : JSON.parse(value);
  };

  const result: any = {};
  Object.keys(config).forEach((group_name) => {
    const fields = config[group_name];
    if (!fields || Object.keys(fields).length === 0) {
      return;
    }

    if (group_name === "globals") {
      Object.keys(fields).forEach((field_name) => {
        result[field_name] = convertValue(fields[field_name]);
      });
    } else {
      result[group_name] = {};

      Object.keys(fields).forEach((field_name) => {
        result[group_name][field_name] = convertValue(fields[field_name]);
      });
    }
  });

  return result;
}
