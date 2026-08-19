#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const files = execFileSync("git", ["ls-files", "-co", "--exclude-standard"], {
  cwd: root,
  encoding: "utf8",
}).trim().split("\n").filter(Boolean);
const detectors = [
  ["private-key", /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/],
  ["aws-access-key", /\bAKIA[0-9A-Z]{16}\b/],
  ["github-token", /\bgh[pousr]_[A-Za-z0-9]{30,}\b/],
  ["jwt", /\beyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\b/],
];
const findings = [];
for (const file of files) {
  let content;
  try {
    content = readFileSync(resolve(root, file), "utf8");
  } catch {
    continue;
  }
  if (content.includes("\0") || content.length > 2_000_000) continue;
  for (const [kind, detector] of detectors) {
    if (detector.test(content)) findings.push({ file, kind });
  }
}
if (findings.length) {
  process.stderr.write(`${JSON.stringify({ result: "failed", findings })}\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({ result: "passed", files_scanned: files.length })}\n`);
