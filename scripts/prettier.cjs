const { spawnSync } = require("node:child_process");
const path = require("node:path");

const root = path.resolve(__dirname, "..");

function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: "inherit" });
  if (result.error) {
    console.error(result.error.message);
    process.exit(1);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

if (process.platform === "win32") {
  run(process.env.ComSpec || "cmd.exe", [
    "/d",
    "/s",
    "/c",
    "npm ci --ignore-scripts --no-audit --no-fund",
  ]);
} else {
  run("npm", ["ci", "--ignore-scripts", "--no-audit", "--no-fund"]);
}

run(process.execPath, [
  path.join(root, "node_modules", "prettier", "bin", "prettier.cjs"),
  ...process.argv.slice(2),
]);
