import lzstring from "lz-string";
import { OptionGroup } from "../ruff_options";

export type Config = { [K: string]: any };

/**
 * Parse an encoded value from the options export.
 *
 * TODO(charlie): Use JSON for the default values.
 */
function parse(value: any): any {
  if (value == "None") {
    return null;
  }
  return JSON.parse(value);
}

/**
 * The default configuration for the playground.
 */
export function defaultConfig(availableOptions: OptionGroup[]): Config {
  const config: Config = {};
  for (const group of availableOptions) {
    if (group.name == "globals") {
      for (const field of group.fields) {
        config[field.name] = parse(field.default);
      }
    } else {
      config[group.name] = {};
      for (const field of group.fields) {
        config[group.name][field.name] = parse(field.default);
      }
    }
  }
  return config;
}

/**
 * Persist the configuration to a URL.
 */
export function persist(configSource: string, pythonSource: string) {
  window.location.hash = lzstring.compressToEncodedURIComponent(
    configSource + "$$$" + pythonSource
  );
}

/**
 * Restore the configuration from a URL.
 */
export function restore(): [string, string] | null {
  const value = lzstring.decompressFromEncodedURIComponent(
    window.location.hash.slice(1)
  );

  if (value) {
    const parts = value.split("$$$");
    const configSource = parts[0];
    const pythonSource = parts[1];
    return [configSource, pythonSource];
  } else {
    return null;
  }
}
