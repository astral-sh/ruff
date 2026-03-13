/**
 * Get the appropriate markdown language tag for a file based on its extension.
 */
function getLanguageTag(filename: string): string {
  const extension = filename.split(".").pop()?.toLowerCase();
  switch (extension) {
    case "py":
    case "pyi":
      return "py";
    case "json":
      return "json";
    case "toml":
      return "toml";
    default:
      return extension ?? "";
  }
}

/**
 * Generate a markdown link to the playground.
 *
 * @param shareUrl - The shareable playground URL
 * @returns Formatted markdown link string
 */
export function generatePlaygroundMarkdownLink(shareUrl: string): string {
  return `[Playground](${shareUrl})`;
}

/**
 * Generate markdown representation of playground files with a shareable link.
 *
 * @param files - Object mapping filenames to their content
 * @param shareUrl - The shareable playground URL
 * @returns Formatted markdown string
 */
export function generatePlaygroundMarkdown(
  files: { [name: string]: string },
  shareUrl: string,
): string {
  let markdown = `## [Playground](${shareUrl})

`;

  for (const [filename, content] of Object.entries(files)) {
    markdown += `### \`${filename}\`

\`\`\`${getLanguageTag(filename)}
${content}
\`\`\`

`;
  }

  return markdown;
}
