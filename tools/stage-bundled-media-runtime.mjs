import {
  chmodSync,
  copyFileSync,
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  renameSync,
  rmSync,
} from "node:fs";
import { createHash } from "node:crypto";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const lockPath = join(root, "apps/desktop/src-tauri/media-runtime.lock");
const lock = Object.fromEntries(
  readFileSync(lockPath, "utf8")
    .split(/\r?\n/)
    .filter(Boolean)
    .map((line) => {
      const index = line.indexOf("=");
      if (index <= 0) throw new Error(`Invalid media-runtime.lock line: ${line}`);
      return [line.slice(0, index), line.slice(index + 1)];
    }),
);

if (lock.format_version !== "1") throw new Error("Unsupported media runtime lock format");
const candidateDir = process.argv[2] || process.env.ACCORDMESH_MEDIA_RUNTIME_CANDIDATE_DIR;
if (!candidateDir) {
  throw new Error(
    "Pass the verified R1B candidate directory or set ACCORDMESH_MEDIA_RUNTIME_CANDIDATE_DIR.",
  );
}

const resolvedCandidateDir = realpathSync(resolve(candidateDir));
const buildInfoPath = join(resolvedCandidateDir, "FFMPEG_BUILD_INFO.json");
const candidateManifestPath = join(resolvedCandidateDir, "SHA256SUMS");
if (!existsSync(buildInfoPath) || !existsSync(candidateManifestPath)) {
  throw new Error("The candidate is missing FFMPEG_BUILD_INFO.json or SHA256SUMS.");
}
const buildInfo = JSON.parse(readFileSync(buildInfoPath, "utf8"));
if (
  buildInfo.schemaVersion !== 2 ||
  buildInfo.version !== lock.ffmpeg_version ||
  buildInfo.target !== lock.target ||
  buildInfo.minimumMacOS !== lock.minimum_macos ||
  buildInfo.pathHygiene !== "PASS" ||
  buildInfo.configuredPrefix !== "/" ||
  buildInfo.networkEnabled !== false ||
  buildInfo.externalAutodetectionEnabled !== false ||
  buildInfo.licensePath !== "LGPL-2.1-or-later; CONFIG_GPL=0; CONFIG_NONFREE=0"
) {
  throw new Error("The candidate build information does not match the approved runtime lock.");
}
const candidateManifest = new Map(
  readFileSync(candidateManifestPath, "utf8")
    .split(/\r?\n/)
    .filter(Boolean)
    .map((line) => {
      const match = line.match(/^([0-9a-f]{64})\s+(.+)$/);
      if (!match) throw new Error(`Invalid candidate SHA256SUMS line: ${line}`);
      return [match[2].trim(), match[1]];
    }),
);

const targetDir = join(root, "apps/desktop/src-tauri/binaries");
mkdirSync(targetDir, { recursive: true });

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

for (const name of ["ffmpeg", "ffprobe"]) {
  const sourceName = `${name}-${lock.target}`;
  const source = join(resolvedCandidateDir, sourceName);
  const expected = lock[`${name}_sha256`];
  if (!existsSync(source)) throw new Error(`Missing verified candidate: ${sourceName}`);
  const metadata = lstatSync(source);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error(`Candidate must be a regular non-symlink file: ${sourceName}`);
  }
  if (candidateManifest.get(sourceName) !== expected) {
    throw new Error(`Candidate SHA256SUMS does not match media-runtime.lock for ${sourceName}`);
  }
  if (buildInfo.binaries?.[sourceName]?.sha256 !== expected) {
    throw new Error(`FFMPEG_BUILD_INFO.json does not match media-runtime.lock for ${sourceName}`);
  }
  const actual = sha256(source);
  if (actual !== expected) {
    throw new Error(`${sourceName} SHA-256 mismatch: expected ${expected}, got ${actual}`);
  }
  const destination = join(targetDir, sourceName);
  const temporary = `${destination}.tmp-${process.pid}`;
  rmSync(temporary, { force: true });
  copyFileSync(source, temporary);
  chmodSync(temporary, 0o755);
  renameSync(temporary, destination);
  const staged = sha256(destination);
  if (staged !== expected) throw new Error(`Staged ${sourceName} failed verification`);
  console.log(`Staged verified media runtime: ${destination}`);
}
