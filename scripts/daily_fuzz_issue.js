/**
 * @file Create a GitHub issue when a daily py-fuzzer job fails.
 *
 * Used with actions/github-script. The caller supplies the issue title, labels,
 * failure description, build instructions, and reproduction command. Include
 * reproducers from fuzz-results/fuzz-results.json when available.
 */
module.exports = async ({
  github,
  context,
  core,
  title,
  labels,
  failureDescription,
  buildInstructions,
  reproductionCommand,
}) => {
  const fs = require("node:fs");
  const runUrl = `https://github.com/${context.repo.owner}/${context.repo.repo}/actions/runs/${context.runId}`;
  let body = `Run listed here: ${runUrl}`;
  try {
    const { bugs } = JSON.parse(
      fs.readFileSync("fuzz-results/fuzz-results.json", "utf8"),
    );
    const reproducers = new Map();
    for (const bug of bugs) {
      const key = JSON.stringify([bug.reproducer, bug.minimization_succeeded]);
      if (!reproducers.has(key)) {
        reproducers.set(key, { ...bug, seeds: [] });
      }
      reproducers.get(key).seeds.push(bug.seed);
    }

    const instructions = [
      "",
      "",
      failureDescription,
      "",
      `${buildInstructions}, save a snippet as \`repro.py\`, and run:`,
      "",
      "```shell",
      reproductionCommand,
      "```",
    ].join("\n");
    const omittedNote =
      "\n\nSome reproducers were omitted to fit the issue body. " +
      "The full results are available in the workflow run's `daily-fuzz-results` artifact.";
    let sections = "";
    let omitted = false;
    for (const {
      reproducer,
      minimization_succeeded,
      seeds,
    } of reproducers.values()) {
      const longestFence = (reproducer.match(/`+/g) || []).reduce(
        (n, run) => Math.max(n, run.length),
        2,
      );
      const fence = "`".repeat(longestFence + 1);
      const status = minimization_succeeded
        ? ""
        : " (original source; minimization did not complete)";
      const heading = `### Seed${seeds.length === 1 ? "" : "s"}: ${seeds.join(", ")}${status}`;
      const section = `\n\n${heading}\n\n${fence}python\n${reproducer}\n${fence}`;
      const size = Buffer.byteLength(
        body + instructions + sections + section + omittedNote,
        "utf8",
      );
      if (size <= 60000) {
        sections += section;
      } else {
        omitted = true;
      }
    }
    if (sections) body += instructions + sections;
    if (omitted) body += omittedNote;
  } catch (error) {
    core.warning(`Fuzz results are unavailable: ${error.message}`);
    body += "\n\nFuzz results were unavailable; check the workflow logs.";
  }
  await github.rest.issues.create({
    owner: context.repo.owner,
    repo: context.repo.repo,
    title,
    body,
    labels,
  });
};
