import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const root = process.cwd();
const ignoredDirectories = new Set([".git", "node_modules", ".pnpm-store", "target", "dist", "build", "coverage"]);
const forbiddenExtensions = new Set([
  ".pem", ".key", ".p12", ".pfx", ".mobileprovision", ".sqlite", ".sqlite3", ".db",
  ".wav", ".mp3", ".m4a", ".aac", ".flac", ".mp4", ".mov", ".mkv", ".webm", ".log",
]);
const requiredFiles = [
  "README.md", "LICENSE", "SECURITY.md", "CONTRIBUTING.md", "CODE_OF_CONDUCT.md",
  "ROADMAP.md", "CHANGELOG.md", "THIRD_PARTY_NOTICES.md",
];
const textExtensions = new Set([".md", ".txt", ".json", ".jsonc", ".toml", ".yml", ".yaml", ".rs", ".ts", ".tsx", ".js", ".mjs", ".css", ".html", ".py", ".swift", ".sql"]);
const findings = [];

function walk(directory) {
  const files = [];
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (entry.isDirectory() && ignoredDirectories.has(entry.name)) continue;
    const full = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...walk(full));
    else if (entry.isFile()) files.push(full);
    else findings.push(`symlink-or-special-file: ${path.relative(root, full)}`);
  }
  return files;
}

for (const required of requiredFiles) {
  if (!fs.existsSync(path.join(root, required))) findings.push(`missing-required-file: ${required}`);
}

const files = walk(root);
for (const file of files) {
  const relative = path.relative(root, file).replaceAll(path.sep, "/");
  const extension = path.extname(file).toLowerCase();
  const base = path.basename(file);
  if (forbiddenExtensions.has(extension)) findings.push(`forbidden-release-file: ${relative}`);
  if (base === ".env" || base.startsWith(".env.")) findings.push(`environment-file: ${relative}`);
  if (textExtensions.has(extension) || [".gitignore", "LICENSE"].includes(base)) {
    const text = fs.readFileSync(file, "utf8");
    const patterns = [
      [/\/Users\/(?!example(?:\/|$))/g, "personal-macos-path"],
      [/\/home\/(?!example(?:\/|$))/g, "personal-linux-path"],
      [/-----BEGIN (?:[A-Z ]+ )?PRIVATE KEY-----/g, "private-key"],
      [/\bsk-[A-Za-z0-9_-]{20,}\b/g, "openai-key-like-value"],
      [/\bgh[pousr]_[A-Za-z0-9_]{20,}\b/g, "github-token-like-value"],
      [/\bAKIA[0-9A-Z]{16}\b/g, "aws-key-like-value"],
      [/OPENAI_API_KEY\s*[:=]\s*[^\s"']+/g, "assigned-openai-key"],
      [/docs\/V0_1_IMPLEMENTATION_HANDOFF\.md/g, "private-handoff-reference"],
      [/\bAGENTS\.md\b/g, "private-agent-instruction-reference"],
      [/\bAFP\b/g, "unexplained-internal-project-reference"],
      [/sanitized\s+public\s+(?:repository|candidate)/gi, "internal-publication-workflow-language"],
      [/pre-publication\s+development\s+repository/gi, "internal-publication-workflow-language"],
      [/long-running\s+development\s+repository/gi, "internal-publication-workflow-language"],
      [/public[-]candidate/gi, "internal-publication-workflow-language"],
      [/private\s+channel\s+listed\s+on\s+the\s+maintainer's\s+GitHub\s+profile/gi, "unavailable-security-contact-reference"],
      [/Use\s+GitHub\s+Discussions\s+for\s+general\s+questions/gi, "disabled-discussions-reference"],
      [/legacy\s+wrapped-key\s+records/gi, "internal-migration-detail"],
      [/permanently\s+deletes\s+all\s+AccordMesh\s+data\s+stored\s+on\s+this\s+device/gi, "overbroad-reset-claim"],
      [/choose\s+one\s+or\s+more\s+related\s+files/gi, "unsupported-multi-file-upload-claim"],
      [/submit\s+the\s+encrypted\s+chunk\s+to\s+the\s+live\s+Provider\s+worker/gi, "misleading-provider-chunk-claim"],
      [/settings\.version",\s*\{\s*version:\s*"0\.1\.0"\s*\}/g, "stale-hardcoded-ui-version"],
      [/appVersion:\s*"0\.1\.0"/g, "stale-hardcoded-artifact-version"],
    ];
    for (const [pattern, label] of patterns) {
      if (pattern.test(text)) findings.push(`${label}: ${relative}`);
    }

    if (extension === ".md") {
      for (const match of text.matchAll(/\[[^\]]+\]\(([^)#]+)(?:#[^)]+)?\)/g)) {
        const target = match[1];
        if (/^[a-z]+:/i.test(target)) continue;
        const resolved = path.resolve(path.dirname(file), target);
        if (!fs.existsSync(resolved)) findings.push(`broken-markdown-link: ${relative} -> ${target}`);
      }
    }
  }
}

if (findings.length) {
  console.error("AccordMesh public release audit: FAIL");
  for (const finding of [...new Set(findings)].sort()) console.error(`- ${finding}`);
  process.exit(1);
}
console.log(`AccordMesh public release audit: PASS (${files.length} files checked)`);
