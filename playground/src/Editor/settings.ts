import lzstring from "lz-string";
import { OptionGroup } from "../ruff_options";

export type Settings = { [K: string]: any };

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
 * The default settings for the playground.
 */
export function defaultSettings(availableOptions: OptionGroup[]): Settings {
  const settings: Settings = {};
  for (const group of availableOptions) {
    if (group.name == "globals") {
      for (const field of group.fields) {
        settings[field.name] = parse(field.default);
      }
    } else {
      settings[group.name] = {};
      for (const field of group.fields) {
        settings[group.name][field.name] = parse(field.default);
      }
    }
  }
  return settings;
}

/**
 * Persist the configuration to a URL.
 */
export function persist(settingsSource: string, pythonSource: string) {
  window.location.hash = lzstring.compressToEncodedURIComponent(
    settingsSource + "$$$" + pythonSource
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
    const settingsSource = parts[0];
    const pythonSource = parts[1];
    return [settingsSource, pythonSource];
  } else {
    return null;
  }
}
