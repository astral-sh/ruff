import { unzipSync } from "fflate";
import { version as pyodideVersion } from "pyodide";
import type { Workspace } from "ty_wasm";

export type PackageKind = "pure-python" | "stubs-only" | "runtime-only";

export interface InstalledPackageInfo {
  name: string;
  version: string;
  kind: PackageKind;
  stubsSource?: string;
}

export interface InstallationStatus {
  state: "idle" | "installing" | "success" | "error";
  message: string;
  /** 0–100 progress percentage, or null when indeterminate (e.g. resolving) */
  progress: number | null;
  installedPackages: InstalledPackageInfo[];
  writtenFiles: string[];
  warnings: string[];
}

export const IDLE_STATUS: InstallationStatus = {
  state: "idle",
  message: "",
  progress: null,
  installedPackages: [],
  writtenFiles: [],
  warnings: [],
};

export const PACKAGES_ROOT = "/packages";

/**
 * Module-level store of extracted package files for use by Pyodide runtime.
 * Updated each time packages are installed.
 */
let extractedPackageFiles: Array<{ path: string; contents: string }> = [];

export function getExtractedPackageFiles(): Array<{
  path: string;
  contents: string;
}> {
  return extractedPackageFiles;
}

/**
 * C extension packages that need pyodide.loadPackage() at runtime.
 */
let runtimePackages: string[] = [];

export function getRuntimePackages(): string[] {
  return runtimePackages;
}
const PYPI_API = "https://pypi.org/pypi";

/**
 * Cached set of package names available in the Pyodide CDN.
 * Fetched once from pyodide-lock.json (~25 KB gzip) on first use, only when
 * C extension packages are present. The result is cached for the session.
 */
let pyodidePackagesPromise: Promise<Set<string>> | null = null;

function fetchPyodidePackages(): Promise<Set<string>> {
  if (pyodidePackagesPromise == null) {
    pyodidePackagesPromise = (async () => {
      try {
        const url = `https://cdn.jsdelivr.net/pyodide/v${pyodideVersion}/full/pyodide-lock.json`;
        const resp = await fetch(url);
        if (!resp.ok) {
          pyodidePackagesPromise = null;
          return new Set<string>();
        }
        const data = await resp.json();
        return new Set<string>(Object.keys(data.packages ?? {}));
      } catch {
        pyodidePackagesPromise = null;
        return new Set<string>();
      }
    })();
  }
  return pyodidePackagesPromise;
}

interface PyPIInfo {
  info: {
    name: string;
    version: string;
    requires_dist: string[] | null;
  };
  urls: Array<{
    filename: string;
    url: string;
    packagetype: string;
    size?: number;
  }>;
}

interface ResolvedPackage {
  name: string;
  version: string;
  wheelUrl: string | null;
  requiresDist: string[] | null;
  kind: PackageKind;
  stubsSource?: string;
}

/**
 * Installs packages from PyPI into the workspace's memory file system.
 *
 * Flow:
 * 1. Resolve all dependencies (BFS, with depth limit)
 * 2. Download and extract pure-Python wheels
 * 3. Write .py/.pyi files to /packages/ in the MemoryFS
 */
export async function installPackages(
  workspace: Workspace,
  packages: string[],
  pythonVersion: string | null,
  previousFiles: string[],
  onProgress: (status: InstallationStatus) => void,
  signal?: AbortSignal,
): Promise<InstallationStatus> {
  if (packages.length === 0) {
    // Clean up previous files if dependencies were removed
    if (previousFiles.length > 0) {
      workspace.removePackageFiles(previousFiles);
    }
    extractedPackageFiles = [];
    runtimePackages = [];
    return IDLE_STATUS;
  }

  onProgress({
    state: "installing",
    message: "Resolving dependencies...",
    progress: null,
    installedPackages: [],
    writtenFiles: [],
    warnings: [],
  });

  try {
    // Resolve all transitive dependencies
    const { packages: resolved, warnings } = await resolveAllDeps(
      packages,
      pythonVersion,
      signal,
    );

    if (signal?.aborted) {
      return IDLE_STATUS;
    }

    // Remove previously written files
    if (previousFiles.length > 0) {
      workspace.removePackageFiles(previousFiles);
    }

    const allWrittenFiles: string[] = [];
    const allExtractedFiles: Array<{ path: string; contents: string }> = [];
    const installed: InstalledPackageInfo[] = [];
    const total = resolved.length;

    for (let i = 0; i < resolved.length; i++) {
      const pkg = resolved[i];

      if (signal?.aborted) {
        return IDLE_STATUS;
      }

      const pct = Math.round((i / total) * 100);

      onProgress({
        state: "installing",
        message: `Installing ${pkg.name} ${pkg.version}...`,
        progress: pct,
        installedPackages: installed,
        writtenFiles: allWrittenFiles,
        warnings,
      });

      if (pkg.wheelUrl != null) {
        const files = await downloadAndExtractWheel(pkg.wheelUrl, signal);

        if (signal?.aborted) {
          return IDLE_STATUS;
        }

        // For stubs-only, only extract .pyi and py.typed (no .py runtime files)
        let filteredFiles: typeof files;
        if (pkg.kind === "stubs-only") {
          filteredFiles = files.filter(
            (f) => f.path.endsWith(".pyi") || f.path.endsWith("/py.typed"),
          );
          // Wheel had no .pyi files — downgrade to runtime-only
          if (filteredFiles.length === 0) {
            pkg.kind = "runtime-only";
            pkg.stubsSource = undefined;
            warnings.push(
              `No type stubs found for '${pkg.name}' — runtime execution only`,
            );
          }
        } else {
          filteredFiles = files;
        }

        const packageFiles = filteredFiles.map(({ path, contents }) => ({
          path: `${PACKAGES_ROOT}/${path}`,
          contents,
        }));

        if (packageFiles.length > 0) {
          // Always write to ty MemoryFS for type checking
          workspace.writePackageFiles(packageFiles);
          allWrittenFiles.push(...packageFiles.map((f) => f.path));

          // Only store pure-python files for Pyodide FS (stubs don't need runtime)
          if (pkg.kind === "pure-python") {
            allExtractedFiles.push(...packageFiles);
          }
        }
      }

      installed.push({
        name: pkg.name,
        version: pkg.version,
        kind: pkg.kind,
        stubsSource: pkg.stubsSource,
      });
    }

    // Store extracted files for Pyodide runtime (pure-python only)
    extractedPackageFiles = allExtractedFiles;

    // Track C extension packages for pyodide.loadPackage() at runtime,
    // but only those actually available in Pyodide's CDN.
    const candidateRuntimePkgs = resolved.filter(
      (pkg) => pkg.kind !== "pure-python",
    );
    if (candidateRuntimePkgs.length > 0) {
      const pyodidePkgs = await fetchPyodidePackages();
      runtimePackages = [];
      for (const pkg of candidateRuntimePkgs) {
        const normalized = normalizePackageName(pkg.name);
        if (pyodidePkgs.has(normalized)) {
          runtimePackages.push(normalized);
        } else {
          warnings.push(
            `'${pkg.name}' is not available in Pyodide — it cannot be imported at runtime`,
          );
        }
      }
    } else {
      runtimePackages = [];
    }

    const result: InstallationStatus = {
      state: "success",
      message: `Installed ${installed.length} package(s)`,
      progress: 100,
      installedPackages: installed,
      writtenFiles: allWrittenFiles,
      warnings,
    };

    onProgress(result);
    return result;
  } catch (error) {
    const result: InstallationStatus = {
      state: "error",
      message: formatError(error),
      progress: null,
      installedPackages: [],
      writtenFiles: previousFiles,
      warnings: [],
    };
    onProgress(result);
    return result;
  }
}

/**
 * Parse a dependency specifier into name and optional pinned version.
 * Supports: "requests", "requests==2.28.0", "requests>=2.0" (version ignored).
 */
function parseDependencySpec(spec: string): {
  name: string;
  version: string | null;
} {
  const match = spec.match(
    /^([A-Za-z0-9]([A-Za-z0-9._-]*[A-Za-z0-9])?)(?:==(.+))?$/,
  );
  if (match) {
    return { name: match[1], version: match[3] ?? null };
  }
  // Strip other specifiers (>=, ~=, !=, etc.) — just use latest
  const nameOnly = spec.match(/^([A-Za-z0-9]([A-Za-z0-9._-]*[A-Za-z0-9])?)/);
  return { name: nameOnly ? nameOnly[1] : spec, version: null };
}

async function fetchPackageInfo(
  name: string,
  version: string | null,
  signal?: AbortSignal,
): Promise<PyPIInfo> {
  const versionSuffix = version != null ? `/${version}` : "";
  const response = await fetch(
    `${PYPI_API}/${encodeURIComponent(name)}${versionSuffix}/json`,
    { signal },
  );
  if (!response.ok) {
    if (response.status === 404) {
      const detail = version != null ? `'${name}==${version}'` : `'${name}'`;
      throw new Error(`Package ${detail} not found on PyPI`);
    }
    throw new Error(
      `Failed to fetch package info for '${name}': ${response.statusText}`,
    );
  }
  return response.json();
}

/** Preference order for pure-Python wheel selection (higher = better match). */
const enum WheelPriority {
  Incompatible = -1,
  /** py3-none-any */
  AnyPython3 = 0,
  /** cp3-none-any */
  AnyCPython3,
  /** e.g. py312-none-any */
  SpecificPython3,
  /** e.g. cp312-none-any */
  SpecificCPython3,
}

/**
 * Score a single python tag for compatibility with Python 3.
 * Returns Incompatible (-1) if the tag doesn't match.
 */
function pythonTagPriority(
  tag: string,
  targetMinor: string | undefined,
): WheelPriority {
  const m = /^(py|cp)3(\d+)?$/.exec(tag);
  if (m == null) {
    return WheelPriority.Incompatible;
  }
  const isCPython = m[1] === "cp";
  const minor = m[2] as string | undefined;
  if (minor != null && targetMinor != null && minor !== targetMinor) {
    return WheelPriority.Incompatible;
  }
  if (minor != null && targetMinor == null) {
    return WheelPriority.Incompatible;
  }
  if (isCPython) {
    return minor != null
      ? WheelPriority.SpecificCPython3
      : WheelPriority.AnyCPython3;
  }
  return minor != null
    ? WheelPriority.SpecificPython3
    : WheelPriority.AnyPython3;
}

const NONE_ANY_SUFFIX = "-none-any.whl";

/**
 * Find the best compatible pure-Python wheel (abi=none, platform=any).
 * Handles version-specific tags like `py312-none-any` in addition to `py3-none-any`.
 */
function findCompatiblePureWheel(
  info: PyPIInfo,
  pythonVersion: string | null,
): { filename: string; url: string } | null {
  const targetMinor = pythonVersion?.split(".")[1];
  let best: {
    filename: string;
    url: string;
    priority: WheelPriority;
  } | null = null;

  for (const url of info.urls) {
    if (url.packagetype !== "bdist_wheel") {
      continue;
    }
    if (!url.filename.endsWith(NONE_ANY_SUFFIX)) {
      continue;
    }

    // Extract compound python tag:
    // e.g. "requests-2.31.0-py2.py3-none-any.whl" → "py2.py3"
    const stem = url.filename.slice(0, -NONE_ANY_SUFFIX.length);
    const pythonTagStr = stem.slice(stem.lastIndexOf("-") + 1);

    let priority: WheelPriority = WheelPriority.Incompatible;
    for (const tag of pythonTagStr.split(".")) {
      const p = pythonTagPriority(tag, targetMinor);
      if (p > priority) {
        priority = p;
      }
    }

    if (
      priority > WheelPriority.Incompatible &&
      (best == null || priority > best.priority)
    ) {
      best = { filename: url.filename, url: url.url, priority };
    }
  }

  return best != null ? { filename: best.filename, url: best.url } : null;
}

interface StubResolution {
  kind: PackageKind;
  wheelUrl: string | null;
  stubsSource?: string;
}

/**
 * Try to find type stubs for a C extension package.
 *
 * Strategy (in priority order):
 * 1. {name}-stubs package on PyPI (e.g. pandas-stubs)
 * 2. types-{name} package on PyPI (e.g. types-psutil)
 * 3. Extract .pyi files from the smallest available wheel
 * 4. Give up → runtime-only
 */
async function resolveStubs(
  name: string,
  info: PyPIInfo,
  pythonVersion: string | null,
  signal?: AbortSignal,
): Promise<StubResolution> {
  // Fetch {name}-stubs and types-{name} in parallel
  const stubsCandidates = [`${name}-stubs`, `types-${name}`];
  const results = await Promise.allSettled(
    stubsCandidates.map((pkg) => fetchPackageInfo(pkg, null, signal)),
  );

  for (let i = 0; i < results.length; i++) {
    const result = results[i];
    if (result.status === "fulfilled") {
      const wheel = findCompatiblePureWheel(result.value, pythonVersion);
      if (wheel != null) {
        return {
          kind: "stubs-only",
          wheelUrl: wheel.url,
          stubsSource: `${stubsCandidates[i]} ${result.value.info.version}`,
        };
      }
    }
  }

  // Strategy 3: Try extracting .pyi from the smallest wheel of the original package.
  // Whether the wheel actually contains .pyi files is verified at install time —
  // if none are found, the package is downgraded to runtime-only.
  const smallestWheel = findSmallestWheel(info);
  if (smallestWheel != null) {
    return {
      kind: "stubs-only",
      wheelUrl: smallestWheel.url,
      stubsSource: "from wheel",
    };
  }

  // Strategy 4: No stubs available
  return { kind: "runtime-only", wheelUrl: null };
}

/**
 * Find the smallest wheel (by file size) for a package, regardless of platform.
 * Used to extract .pyi type stubs from C extension packages.
 */
function findSmallestWheel(
  info: PyPIInfo,
): { filename: string; url: string } | null {
  let best: { filename: string; url: string; size: number } | null = null;
  for (const url of info.urls) {
    if (url.packagetype !== "bdist_wheel") {
      continue;
    }
    const size = url.size ?? Number.MAX_SAFE_INTEGER;
    if (best == null || size < best.size) {
      best = { filename: url.filename, url: url.url, size };
    }
  }
  return best != null ? { filename: best.filename, url: best.url } : null;
}

const textDecoder = new TextDecoder();

async function downloadAndExtractWheel(
  url: string,
  signal?: AbortSignal,
): Promise<Array<{ path: string; contents: string }>> {
  const response = await fetch(url, { signal });
  if (!response.ok) {
    throw new Error(`Failed to download wheel: ${response.statusText}`);
  }

  const buffer = await response.arrayBuffer();
  const zipData = new Uint8Array(buffer);

  // Filter during decompression so binary files (.so, .dll, etc.) are never inflated
  const unzipped = unzipSync(zipData, {
    filter: (file) =>
      !file.name.includes(".dist-info/") &&
      (file.name.endsWith(".py") ||
        file.name.endsWith(".pyi") ||
        file.name.endsWith("/py.typed")),
  });

  const files: Array<{ path: string; contents: string }> = [];

  for (const [path, data] of Object.entries(unzipped)) {
    files.push({
      path,
      contents: textDecoder.decode(data),
    });
  }

  return files;
}

interface VersionConstraint {
  op: "==" | "!=" | ">=" | "<=" | ">" | "<" | "~=";
  version: string;
}

interface DependencyWithConstraints {
  name: string;
  constraints: VersionConstraint[];
}

/**
 * Parse version constraint string like ">=2.0,<4.0,!=3.0.1" into individual constraints.
 */
function parseVersionConstraints(specifier: string): VersionConstraint[] {
  if (specifier.length === 0) {
    return [];
  }

  const constraints: VersionConstraint[] = [];
  const parts = specifier.split(",");

  for (const part of parts) {
    const trimmed = part.trim();
    const match = trimmed.match(/^(~=|==|!=|>=|<=|>|<)\s*([A-Za-z0-9.*_-]+)$/);
    if (match) {
      constraints.push({
        op: match[1] as VersionConstraint["op"],
        version: match[2],
      });
    }
  }

  return constraints;
}

/**
 * Compare two PEP 440 version strings numerically.
 * Returns negative if a < b, 0 if equal, positive if a > b.
 */
function compareVersions(a: string, b: string): number {
  const partsA = a.split(".").map((s) => parseInt(s, 10) || 0);
  const partsB = b.split(".").map((s) => parseInt(s, 10) || 0);
  const len = Math.max(partsA.length, partsB.length);

  for (let i = 0; i < len; i++) {
    const numA = partsA[i] ?? 0;
    const numB = partsB[i] ?? 0;
    if (numA !== numB) {
      return numA - numB;
    }
  }
  return 0;
}

/**
 * Check if a resolved version satisfies a single version constraint.
 */
function satisfiesConstraint(
  resolved: string,
  constraint: VersionConstraint,
): boolean {
  // Wildcard matches (e.g. ==3.*) — just check prefix
  if (constraint.version.includes("*")) {
    const prefix = constraint.version.replace(/\.\*$/, "");
    const resolvedPrefix = resolved
      .split(".")
      .slice(0, prefix.split(".").length)
      .join(".");
    const matches = compareVersions(resolvedPrefix, prefix) === 0;
    return constraint.op === "=="
      ? matches
      : constraint.op === "!="
        ? !matches
        : true;
  }

  const cmp = compareVersions(resolved, constraint.version);

  switch (constraint.op) {
    case "==":
      return cmp === 0;
    case "!=":
      return cmp !== 0;
    case ">=":
      return cmp >= 0;
    case "<=":
      return cmp <= 0;
    case ">":
      return cmp > 0;
    case "<":
      return cmp < 0;
    case "~=":
      // ~=X.Y is equivalent to >=X.Y,<(X+1).0
      // ~=X.Y.Z is equivalent to >=X.Y.Z,<X.(Y+1).0
      if (cmp < 0) {
        return false;
      }
      {
        const parts = constraint.version.split(".");
        const upperParts = parts.slice(0, -1);
        upperParts[upperParts.length - 1] = String(
          parseInt(upperParts[upperParts.length - 1], 10) + 1,
        );
        return compareVersions(resolved, upperParts.join(".")) < 0;
      }
  }
}

/**
 * Format version constraints as a human-readable string.
 */
function formatConstraints(constraints: VersionConstraint[]): string {
  return constraints.map((c) => `${c.op}${c.version}`).join(",");
}

function normalizePackageName(name: string): string {
  return name.toLowerCase().replace(/[-_.]+/g, "-");
}

/**
 * Check whether a resolved version satisfies all constraints, and push a warning if not.
 */
function checkConstraints(
  name: string,
  constraints: VersionConstraint[],
  resolvedVersion: string,
  requestedBy: string | null,
  warnings: string[],
): void {
  for (const constraint of constraints) {
    if (!satisfiesConstraint(resolvedVersion, constraint)) {
      const source = requestedBy != null ? ` (required by ${requestedBy})` : "";
      warnings.push(
        `${name} ${formatConstraints(constraints)}${source} conflicts with installed ${resolvedVersion}`,
      );
      break;
    }
  }
}

/**
 * Parse mandatory dependencies from requires_dist, excluding extras.
 * Returns package names with their version constraints.
 */
function parseMandatoryDeps(
  requiresDist: string[] | null,
): DependencyWithConstraints[] {
  if (requiresDist == null) {
    return [];
  }

  const deps: DependencyWithConstraints[] = [];

  for (const spec of requiresDist) {
    // Skip entries with extra conditions like: 'foo ; extra == "dev"'
    if (/extra\s*==/.test(spec)) {
      continue;
    }
    // Extract the package name (before any version specifier or semicolon)
    const match = spec.match(/^([A-Za-z0-9]([A-Za-z0-9._-]*[A-Za-z0-9])?)/);
    if (match) {
      const name = normalizePackageName(match[1]);
      // Extract version constraints from the remainder (before any semicolon for env markers)
      const afterName = spec.slice(match[0].length).split(";")[0].trim();
      const constraints = parseVersionConstraints(afterName);
      deps.push({ name, constraints });
    }
  }

  return deps;
}

/**
 * Resolve all transitive dependencies using BFS with depth limit.
 */
interface ResolveResult {
  packages: ResolvedPackage[];
  warnings: string[];
}

async function resolveAllDeps(
  packages: string[],
  pythonVersion: string | null,
  signal?: AbortSignal,
): Promise<ResolveResult> {
  const resolved: ResolvedPackage[] = [];
  const warnings: string[] = [];
  // Maps normalized name → installed version string
  const seen = new Map<string, string>();

  interface QueueEntry {
    name: string;
    version: string | null;
    constraints: VersionConstraint[];
    depth: number;
    requestedBy: string | null;
  }

  const queue: QueueEntry[] = packages.map((spec) => {
    const parsed = parseDependencySpec(spec);
    return {
      name: normalizePackageName(parsed.name),
      version: parsed.version,
      constraints:
        parsed.version != null
          ? [{ op: "==" as const, version: parsed.version }]
          : [],
      depth: 0,
      requestedBy: null,
    };
  });

  while (queue.length > 0) {
    if (signal?.aborted) {
      return { packages: resolved, warnings };
    }

    const entry = queue.shift()!;
    // entry.name is already normalized (by queue producers)
    const existingVersion = seen.get(entry.name);
    if (existingVersion != null) {
      checkConstraints(
        entry.name,
        entry.constraints,
        existingVersion,
        entry.requestedBy,
        warnings,
      );
      continue;
    }

    let info: PyPIInfo;
    try {
      info = await fetchPackageInfo(entry.name, entry.version, signal);
    } catch (error) {
      // If this is a top-level package, propagate the error
      if (entry.depth === 0) {
        throw error;
      }
      // For transitive deps, just skip if not found
      continue;
    }

    // Validate fetched version against constraints (e.g. >=1,<3).
    // We always fetch the latest (or pinned) version; warn if that doesn't
    // satisfy a transitive constraint.
    checkConstraints(
      entry.name,
      entry.constraints,
      info.info.version,
      entry.requestedBy,
      warnings,
    );

    const wheel = findCompatiblePureWheel(info, pythonVersion);
    if (wheel != null) {
      // Pure-Python package
      seen.set(entry.name, info.info.version);
      resolved.push({
        name: info.info.name,
        version: info.info.version,
        wheelUrl: wheel.url,
        requiresDist: info.info.requires_dist,
        kind: "pure-python",
      });
    } else {
      // C extension package (any depth): try to find type stubs
      const stubs = await resolveStubs(entry.name, info, pythonVersion, signal);
      if (signal?.aborted) {
        return { packages: resolved, warnings };
      }
      seen.set(entry.name, info.info.version);
      resolved.push({
        name: info.info.name,
        version: info.info.version,
        wheelUrl: stubs.wheelUrl,
        requiresDist: null,
        kind: stubs.kind,
        stubsSource: stubs.stubsSource,
      });
      if (stubs.kind === "runtime-only") {
        warnings.push(
          `No type stubs found for '${entry.name}': runtime execution only`,
        );
      }
      // Do not enqueue transitive deps: Pyodide handles them
      continue;
    }

    // Enqueue transitive dependencies — only for pure-python
    const deps = parseMandatoryDeps(info.info.requires_dist);
    for (const dep of deps) {
      // Find the exact pin version if there is one (==X.Y.Z)
      const exactPin = dep.constraints.find(
        (c) => c.op === "==" && !c.version.includes("*"),
      );

      if (!seen.has(dep.name)) {
        queue.push({
          name: dep.name,
          version: exactPin?.version ?? null,
          constraints: dep.constraints,
          depth: entry.depth + 1,
          requestedBy: info.info.name,
        });
      } else {
        checkConstraints(
          dep.name,
          dep.constraints,
          seen.get(dep.name)!,
          info.info.name,
          warnings,
        );
      }
    }
  }

  return { packages: resolved, warnings };
}

export function formatError(error: unknown): string {
  const message = error instanceof Error ? error.message : `${error}`;
  return message.startsWith("Error: ")
    ? message.slice("Error: ".length)
    : message;
}
