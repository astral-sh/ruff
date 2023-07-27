import lzstring from "lz-string";

export type Settings = { [K: string]: any };

/**
 * Stringify a settings object to JSON.
 */
export function stringify(settings: Settings): string {
  return JSON.stringify(
    settings,
    (k, v) => {
      if (v instanceof Map) {
        return Object.fromEntries(v.entries());
      } else {
        return v;
      }
    },
    2,
  );
}

/**
 * Persist the configuration to a URL.
 */
export async function persist(settingsSource: string, pythonSource: string) {
  const hash = lzstring.compressToEncodedURIComponent(
    settingsSource.replaceAll("$$$", "$$$$$$") + "$$$" + pythonSource,
  );
  await navigator.clipboard.writeText(
    window.location.href.split("#")[0] + "#" + hash,
  );
}

/**
 * Restore the configuration from a URL.
 */
export function restore(): [string, string] | null {
  const value = lzstring.decompressFromEncodedURIComponent(
    window.location.hash.slice(1),
  );

  if (value == null) {
    return restoreLocal();
  } else {
    const [settingsSource, pythonSource] = value.split("$$$");
    return [settingsSource.replaceAll("$$$$$$", "$$$"), pythonSource];
  }
}

function restoreLocal(): [string, string] | null {
  const source = localStorage.getItem("source");

  if (source == null) {
    return null;
  } else {
    return JSON.parse(source);
  }
}

export function persistLocal({
  settingsSource,
  pythonSource,
}: {
  settingsSource: string;
  pythonSource: string;
}) {
  localStorage.setItem(
    "source",
    JSON.stringify([settingsSource, pythonSource]),
  );
}
