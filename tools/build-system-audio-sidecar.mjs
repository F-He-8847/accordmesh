import {
  chmodSync,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const HELPER_BUILD_CACHE_VERSION = "accordmesh-system-audio-v2";
const HELPER_BUILD_ARGS = [
  "-O", "-parse-as-library",
  "-framework", "Foundation",
  "-framework", "ScreenCaptureKit",
  "-framework", "CoreMedia",
  "-framework", "CoreAudio",
  "-framework", "CoreGraphics",
];
const HELPER_BUILD_FLAGS = HELPER_BUILD_ARGS.join("|");

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
if (process.platform !== "darwin") {
  console.log("System-audio sidecar build skipped on non-macOS host.");
  process.exit(0);
}

const triple = process.arch === "arm64"
  ? "aarch64-apple-darwin"
  : process.arch === "x64"
    ? "x86_64-apple-darwin"
    : null;
if (!triple) {
  throw new Error(`Unsupported macOS architecture: ${process.arch}`);
}

const source = join(root, "apps/desktop/src-tauri/src/platform/macos/system_audio.swift");
const outputDir = join(root, "apps/desktop/src-tauri/binaries");
const output = join(outputDir, `accordmesh-system-audio-${triple}`);
const stamp = join(outputDir, `.accordmesh-system-audio-${triple}.build-stamp`);
mkdirSync(outputDir, { recursive: true });

const sourceHash = createHash("sha256").update(readFileSync(source)).digest("hex");
const versionResult = spawnSync("xcrun", ["swiftc", "--version"], { encoding: "utf8" });
if (versionResult.error) throw versionResult.error;
if (versionResult.status !== 0) process.exit(versionResult.status ?? 1);
const swiftVersion = `${versionResult.stdout ?? ""}${versionResult.stderr ?? ""}`
  .replaceAll("\r\n", "\n")
  .trim();
const expectedStamp = [
  HELPER_BUILD_CACHE_VERSION,
  `source_sha256=${sourceHash}`,
  `target=${triple}`,
  `swiftc=${swiftVersion}`,
  `flags=${HELPER_BUILD_FLAGS}`,
  "",
].join("\n");

if (existsSync(output) && existsSync(stamp) && readFileSync(stamp, "utf8") === expectedStamp) {
  if ((statSync(output).mode & 0o111) === 0) chmodSync(output, 0o755);
  console.log(`System-audio sidecar is current; skipped rebuild: ${output}`);
  process.exit(0);
}

const suffix = `${process.pid}-${Date.now()}`;
const temporaryOutput = join(outputDir, `.accordmesh-system-audio-${triple}.tmp-${suffix}`);
const temporaryStamp = join(outputDir, `.accordmesh-system-audio-${triple}.build-stamp.tmp-${suffix}`);
rmSync(temporaryOutput, { force: true });
rmSync(temporaryStamp, { force: true });

const result = spawnSync("xcrun", [
  "swiftc", "-O", "-parse-as-library", source, "-o", temporaryOutput,
  "-framework", "Foundation",
  "-framework", "ScreenCaptureKit",
  "-framework", "CoreMedia",
  "-framework", "CoreAudio",
  "-framework", "CoreGraphics",
], { stdio: "inherit" });
if (result.error) throw result.error;
if (result.status !== 0) {
  rmSync(temporaryOutput, { force: true });
  process.exit(result.status ?? 1);
}

writeFileSync(temporaryStamp, expectedStamp, "utf8");
renameSync(temporaryOutput, output);
renameSync(temporaryStamp, stamp);
chmodSync(output, 0o755);
console.log(`Built system-audio sidecar: ${output}`);
