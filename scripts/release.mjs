#!/usr/bin/env node

/**
 * Uira Release Deployer
 *
 * Single script that handles the entire release lifecycle.
 * The CI workflow calls subcommands at each step; if any step fails,
 * subsequent steps (including the version-bump commit) never run.
 *
 * Usage: node scripts/release.mjs <command> [args]
 *
 * Commands:
 *   bump <patch|minor|major|X.Y.Z>       Bump versions across all files (no commit)
 *   verify                                Verify all versions match
 *   publish-crates                        Publish to crates.io in dependency order
 *   generate-npm <artifacts-dir> <out>    Generate platform npm packages from build artifacts
 *   publish-npm-platforms <dir>           Publish generated platform packages
 *   publish-npm-main                      Publish @uiradev/uira
 *   publish-npm-hook                      Publish @uiradev/hook
 *   release-notes <version>               Generate release notes (stdout)
 *   finalize <version>                    Commit version bump, tag, create GitHub release
 */

import * as fs from "node:fs";
import * as path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

// ── Paths ────────────────────────────────────────────────────────────────────

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "..");
const CARGO_TOML = path.join(ROOT, "Cargo.toml");
const UIRA_PKG = path.join(ROOT, "packages/uira/package.json");
const HOOK_PKG = path.join(ROOT, "packages/hook/package.json");

const REPO = process.env.GITHUB_REPOSITORY || "junhoyeo/uira";

// ── Constants ────────────────────────────────────────────────────────────────

/** Crate publish order — layers separated by comments for clarity. */
const CRATE_ORDER = [
  // Layer 0: no internal dependencies
  "uira-core",
  "uira-oxc",
  "uira-mcp-client",
  "uira-comment-checker",
  // Layer 1: depends on Layer 0
  "uira-providers",
  "uira-security",
  // Layer 2
  "uira-orchestration",
  // Layer 3
  "uira-agent",
  "uira-mcp-server",
  // Layer 4
  "uira-tui",
  "uira-gateway",
  // Layer 5: top-level binaries
  "uira-cli",
  "uira-commit-hook-cli",
];

const PLATFORMS = [
  { codeTarget: "darwin-arm64", os: "darwin", cpu: "arm64", pkg: "@uiradev/uira-darwin-arm64" },
  { codeTarget: "darwin-x64", os: "darwin", cpu: "x64", pkg: "@uiradev/uira-darwin-x64" },
  { codeTarget: "linux-x64-gnu", os: "linux", cpu: "x64", pkg: "@uiradev/uira-linux-x64-gnu", libc: "glibc" },
  { codeTarget: "linux-arm64-gnu", os: "linux", cpu: "arm64", pkg: "@uiradev/uira-linux-arm64-gnu", libc: "glibc" },
  { codeTarget: "linux-x64-musl", os: "linux", cpu: "x64", pkg: "@uiradev/uira-linux-x64-musl", libc: "musl" },
  { codeTarget: "linux-arm64-musl", os: "linux", cpu: "arm64", pkg: "@uiradev/uira-linux-arm64-musl", libc: "musl" },
  { codeTarget: "win32-x64-msvc", os: "win32", cpu: "x64", pkg: "@uiradev/uira-win32-x64-msvc" },
];

// ── Helpers ──────────────────────────────────────────────────────────────────

function die(msg) {
  console.error(`\x1b[31mError: ${msg}\x1b[0m`);
  process.exit(1);
}

function log(msg) {
  console.log(`  ${msg}`);
}

function heading(msg) {
  console.log(`\n\x1b[1m${msg}\x1b[0m`);
}

/** Run a command, return trimmed stdout. Throws on non-zero exit. */
function run(cmd, args, opts = {}) {
  const { allowFailure = false, cwd = ROOT, silent = false } = opts;
  try {
    const result = execFileSync(cmd, args, {
      encoding: "utf8",
      cwd,
      stdio: silent ? ["ignore", "pipe", "pipe"] : ["ignore", "pipe", "inherit"],
    });
    return result.trim();
  } catch (e) {
    if (allowFailure) return "";
    throw e;
  }
}

/** Run and parse JSON output. Returns null on failure when allowFailure=true. */
function runJson(cmd, args, opts = {}) {
  const out = run(cmd, args, { ...opts, silent: true });
  if (!out) return null;
  try {
    return JSON.parse(out);
  } catch {
    return null;
  }
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf-8"));
}

function writeJson(filePath, data) {
  fs.writeFileSync(filePath, JSON.stringify(data, null, 2) + "\n");
}

function readText(filePath) {
  return fs.readFileSync(filePath, "utf-8");
}

function writeText(filePath, text) {
  fs.writeFileSync(filePath, text);
}

function currentCargoVersion() {
  const text = readText(CARGO_TOML);
  const match = text.match(/^version\s*=\s*"([^"]+)"/m);
  return match?.[1] ?? null;
}

// ── bump ─────────────────────────────────────────────────────────────────────

function bump(input) {
  if (!input) die("Usage: release.mjs bump <patch|minor|major|X.Y.Z>");

  const current = currentCargoVersion();
  if (!current) die("Could not read version from Cargo.toml");

  heading(`Bumping from ${current}`);

  // Calculate new version
  let version;
  if (/^\d+\.\d+\.\d+/.test(input)) {
    version = input;
  } else {
    const base = current.replace(/-.*$/, "");
    const [major, minor, patch] = base.split(".").map(Number);
    switch (input) {
      case "major":
        version = `${major + 1}.0.0`;
        break;
      case "minor":
        version = `${major}.${minor + 1}.0`;
        break;
      case "patch":
        version = `${major}.${minor}.${patch + 1}`;
        break;
      default:
        die(`Unknown bump type: ${input}. Use patch, minor, major, or X.Y.Z`);
    }
  }

  // Validate format
  if (!/^\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?$/.test(version)) {
    die(`Invalid version format: ${version}`);
  }

  console.log(`  New version: ${version}`);

  // 1. Cargo.toml — workspace version
  let cargo = readText(CARGO_TOML);
  cargo = cargo.replace(/^(version\s*=\s*")[^"]+(")/m, `$1${version}$2`);

  // workspace dependency versions: update if present, add if missing
  cargo = cargo.replace(
    /(uira-[a-z-]+\s*=\s*\{\s*path\s*=\s*"crates\/[^"]+")(?:,\s*version\s*=\s*"[^"]+")?(\s*})/g,
    `$1, version = "${version}"$2`,
  );
  writeText(CARGO_TOML, cargo);
  log("Cargo.toml (workspace version + dependency versions)");

  // 2. packages/uira/package.json
  const uiraPkg = readJson(UIRA_PKG);
  uiraPkg.version = version;
  if (uiraPkg.optionalDependencies) {
    for (const dep of Object.keys(uiraPkg.optionalDependencies)) {
      if (dep.startsWith("@uiradev/")) {
        uiraPkg.optionalDependencies[dep] = version;
      }
    }
  }
  writeJson(UIRA_PKG, uiraPkg);
  log("packages/uira/package.json");

  // 3. packages/hook/package.json (resolve workspace:* → actual version)
  const hookPkg = readJson(HOOK_PKG);
  hookPkg.version = version;
  if (hookPkg.dependencies?.["@uiradev/uira"]) {
    hookPkg.dependencies["@uiradev/uira"] = version;
  }
  writeJson(HOOK_PKG, hookPkg);
  log("packages/hook/package.json");

  // Verify
  verify(version);

  // Output for CI
  if (process.env.GITHUB_OUTPUT) {
    fs.appendFileSync(process.env.GITHUB_OUTPUT, `version=${version}\n`);
  }

  console.log(`\nVersion ${version} applied. Files modified, NOT committed.`);
}

// ── verify ───────────────────────────────────────────────────────────────────

function verify(expected) {
  heading("Verifying versions");

  const cargoVer = currentCargoVersion();
  const uiraVer = readJson(UIRA_PKG).version;
  const hookVer = readJson(HOOK_PKG).version;

  log(`Cargo.toml:       ${cargoVer}`);
  log(`packages/uira:    ${uiraVer}`);
  log(`packages/hook:    ${hookVer}`);

  // All three must match each other
  if (cargoVer !== uiraVer || cargoVer !== hookVer) {
    die(`Version mismatch: Cargo.toml=${cargoVer}, uira=${uiraVer}, hook=${hookVer}`);
  }

  // If an expected version was provided, check against it
  if (expected && cargoVer !== expected) {
    die(`Expected ${expected} but found ${cargoVer}`);
  }

  console.log(`  All versions match: ${cargoVer}`);
}

// ── publish-crates ───────────────────────────────────────────────────────────

function publishCrates() {
  heading("Publishing to crates.io");

  for (const crate of CRATE_ORDER) {
    log(`Publishing ${crate}...`);
    run("cargo", ["publish", "-p", crate, "--no-verify"]);
    log(`${crate} published`);

    // Wait for crates.io index to propagate before publishing dependents
    if (crate !== CRATE_ORDER[CRATE_ORDER.length - 1]) {
      log("Waiting 15s for index propagation...");
      execFileSync("sleep", ["15"]);
    }
  }

  console.log("\nAll crates published to crates.io");
}

// ── generate-npm ─────────────────────────────────────────────────────────────

function generateNpm(artifactsDir, outputDir) {
  if (!artifactsDir || !outputDir) {
    die("Usage: release.mjs generate-npm <artifacts-dir> <output-dir>");
  }

  const mainPkg = readJson(UIRA_PKG);
  const version = mainPkg.version;

  heading(`Generating npm platform packages (v${version})`);
  log(`Artifacts: ${artifactsDir}`);
  log(`Output:    ${outputDir}`);

  for (const platform of PLATFORMS) {
    const artifactPath = path.join(artifactsDir, `npm-${platform.codeTarget}`);

    if (!fs.existsSync(artifactPath)) {
      console.warn(`  Skipping ${platform.codeTarget}: no artifacts at ${artifactPath}`);
      continue;
    }

    const pkgDir = path.join(outputDir, platform.codeTarget);
    fs.mkdirSync(pkgDir, { recursive: true });

    const isWindows = platform.os === "win32";
    const ext = isWindows ? ".exe" : "";

    // Package manifest
    const manifest = {
      name: platform.pkg,
      version,
      description: `Platform-specific binary for @uiradev/uira (${platform.codeTarget})`,
      license: mainPkg.license,
      repository: mainPkg.repository,
      os: [platform.os],
      cpu: [platform.cpu],
      publishConfig: { access: "public", provenance: true },
    };
    if (platform.libc) manifest.libc = [platform.libc];

    // Copy binaries
    for (const binary of [`uira-agent${ext}`, `uira-commit-hook-cli${ext}`]) {
      const src = path.join(artifactPath, binary);
      const dest = path.join(pkgDir, binary);

      if (!fs.existsSync(src)) die(`Missing binary: ${src}`);

      fs.copyFileSync(src, dest);
      if (!isWindows) fs.chmodSync(dest, 0o755);
      log(`Copy ${binary} -> ${platform.codeTarget}/`);
    }

    writeJson(path.join(pkgDir, "package.json"), manifest);
    log(`Generated ${platform.pkg}@${version}`);
  }

  // Sync optionalDependencies in main package
  const updatedOptDeps = {};
  for (const p of PLATFORMS) updatedOptDeps[p.pkg] = version;
  mainPkg.optionalDependencies = updatedOptDeps;
  writeJson(UIRA_PKG, mainPkg);
  log(`Updated ${UIRA_PKG} optionalDependencies`);

  console.log(`\nGenerated ${PLATFORMS.length} platform packages.`);
}

// ── publish-npm-platforms ────────────────────────────────────────────────────

function publishNpmPlatforms(dir) {
  if (!dir) die("Usage: release.mjs publish-npm-platforms <dir>");

  heading("Publishing npm platform packages");

  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    const pkgJson = path.join(dir, entry.name, "package.json");
    if (!fs.existsSync(pkgJson)) continue;

    const pkg = readJson(pkgJson);
    log(`Publishing ${pkg.name}@${pkg.version}...`);
    run("npm", ["publish", path.join(dir, entry.name), "--access", "public", "--provenance"]);
    log(`${pkg.name} published`);
  }
}

// ── publish-npm-main ─────────────────────────────────────────────────────────

function publishNpmMain() {
  heading("Publishing @uiradev/uira");

  const pkgDir = path.join(ROOT, "packages/uira");
  run("npm", ["run", "build"], { cwd: pkgDir });
  run("npm", ["publish", "--access", "public", "--provenance"], { cwd: pkgDir });

  const version = readJson(UIRA_PKG).version;
  console.log(`\n@uiradev/uira@${version} published`);
}

// ── publish-npm-hook ─────────────────────────────────────────────────────────

function publishNpmHook() {
  heading("Publishing @uiradev/hook");

  const pkgDir = path.join(ROOT, "packages/hook");
  run("npm", ["run", "build"], { cwd: pkgDir });
  run("npm", ["publish", "--access", "public", "--provenance"], { cwd: pkgDir });

  const version = readJson(HOOK_PKG).version;
  console.log(`\n@uiradev/hook@${version} published`);
}

// ── release-notes ────────────────────────────────────────────────────────────

function releaseNotes(version) {
  if (!version) die("Usage: release.mjs release-notes <version>");

  const prevTag = run("git", ["describe", "--tags", "--abbrev=0", "HEAD^"], {
    allowFailure: true,
    silent: true,
  });

  // Get commits since previous tag
  const commits = getCommitsSince(prevTag);

  // Resolve PRs for each commit
  const entries = [];
  const seenPRs = new Set();
  for (const commit of commits) {
    const pr = findPR(commit.hash);

    if (pr?.number && seenPRs.has(pr.number)) continue;
    if (pr?.number) seenPRs.add(pr.number);

    entries.push({
      hash: commit.hash,
      message: pr?.title || commit.message,
      author: pr ? `@${pr.authorLogin}` : resolveAuthor(commit.email, commit.name),
      prNumber: pr?.number,
    });
  }

  // Build notes
  const lines = [
    `## Uira v${version}`,
    "",
    "### Installation",
    "",
    "```bash",
    "# npm",
    `npx @uiradev/uira@${version} --version`,
    `npm install -g @uiradev/uira@${version}`,
    "",
    "# cargo",
    `cargo install uira-cli@${version}            # uira-agent (main CLI)`,
    `cargo install uira-commit-hook-cli@${version} # git hooks CLI`,
    `cargo install uira-mcp-server@${version}      # MCP server`,
    "```",
    "",
    "### Changes",
    "",
  ];

  if (entries.length === 0) {
    lines.push("* Initial release");
  } else {
    for (const e of entries.reverse()) {
      const prLink = e.prNumber ? ` in https://github.com/${REPO}/pull/${e.prNumber}` : "";
      const hashRef = e.prNumber ? "" : ` (${e.hash.slice(0, 7)})`;
      lines.push(`* ${e.message} by ${e.author}${prLink}${hashRef}`);
    }
  }

  lines.push(
    "",
    "### Binaries",
    "",
    "| Platform | uira-agent | uira-commit-hook-cli | uira-mcp |",
    "|----------|------------|---------------------|----------|",
    "| macOS ARM64 | `uira-agent-darwin-arm64` | `uira-commit-hook-cli-darwin-arm64` | `uira-mcp-darwin-arm64` |",
    "| macOS x64 | `uira-agent-darwin-x64` | `uira-commit-hook-cli-darwin-x64` | `uira-mcp-darwin-x64` |",
    "| Linux x64 | `uira-agent-linux-x64-gnu` | `uira-commit-hook-cli-linux-x64-gnu` | `uira-mcp-linux-x64-gnu` |",
    "| Linux ARM64 | `uira-agent-linux-arm64-gnu` | `uira-commit-hook-cli-linux-arm64-gnu` | `uira-mcp-linux-arm64-gnu` |",
    "| Linux x64 (musl) | `uira-agent-linux-x64-musl` | `uira-commit-hook-cli-linux-x64-musl` | `uira-mcp-linux-x64-musl` |",
    "| Linux ARM64 (musl) | `uira-agent-linux-arm64-musl` | `uira-commit-hook-cli-linux-arm64-musl` | `uira-mcp-linux-arm64-musl` |",
    "| Windows x64 | `uira-agent-win32-x64-msvc.exe` | `uira-commit-hook-cli-win32-x64-msvc.exe` | `uira-mcp-win32-x64-msvc.exe` |",
  );

  if (prevTag) {
    lines.push("", `**Full Changelog**: https://github.com/${REPO}/compare/${prevTag}...v${version}`);
  }

  // Output to stdout (CI pipes this to a file)
  console.log(lines.join("\n"));
}

/** Get non-merge commits since a tag (or all if no tag). */
function getCommitsSince(tag) {
  const range = tag ? `${tag}..HEAD` : "HEAD";
  const raw = run("git", ["log", range, "--format=%H%x1f%s%x1f%an%x1f%ae", "--no-merges"], {
    allowFailure: true,
    silent: true,
  });
  if (!raw) return [];
  return raw
    .split("\n")
    .filter((l) => l.trim())
    .map((l) => {
      const [hash = "", message = "", name = "", email = ""] = l.split("\x1f");
      return { hash, message, name, email };
    })
    .filter((c) => c.hash && !c.message.startsWith("chore: bump version"));
}

/** Look up the PR associated with a commit via `gh`. */
function findPR(hash) {
  const result = runJson("gh", [
    "pr", "list", "--repo", REPO, "--state", "merged",
    "--search", hash, "--json", "number,title,author", "--limit", "1",
  ], { allowFailure: true });
  const pr = result?.[0];
  if (!pr?.number || !pr.author?.login) return null;
  return { number: pr.number, title: pr.title, authorLogin: pr.author.login };
}

/** Resolve a git email to a GitHub @username. */
function resolveAuthor(email, fallback) {
  if (email.includes("@users.noreply.github.com")) {
    const m = email.match(/(?:\d+\+)?([^@]+)@users\.noreply\.github\.com/);
    if (m?.[1]) return `@${m[1]}`;
  }
  const search = runJson("gh", [
    "api", `/search/users?q=${encodeURIComponent(email)}+in:email`,
  ], { allowFailure: true });
  const login = search?.items?.[0]?.login;
  return login ? `@${login}` : fallback;
}

// ── finalize ─────────────────────────────────────────────────────────────────

function finalize(version) {
  if (!version) die("Usage: release.mjs finalize <version>");

  heading(`Finalizing release v${version}`);

  // 1. Commit version bump
  log("Committing version bump...");
  run("git", ["config", "user.name", "github-actions[bot]"]);
  run("git", ["config", "user.email", "github-actions[bot]@users.noreply.github.com"]);
  run("git", ["add", "Cargo.toml", "packages/uira/package.json", "packages/hook/package.json"]);
  run("git", ["commit", "-m", `chore: bump version to ${version}`]);

  // Pull with rebase to handle main advancing during the long publish jobs
  run("git", ["pull", "--rebase", "origin", "main"]);
  run("git", ["push"]);
  log("Version bump committed and pushed");

  // 2. Create tag
  const tagExists = run("git", ["rev-parse", `v${version}`], { allowFailure: true, silent: true });
  if (tagExists) {
    log(`Tag v${version} already exists`);
  } else {
    run("git", ["tag", `v${version}`]);
    run("git", ["push", "origin", `v${version}`]);
    log(`Created tag v${version}`);
  }

  // 3. Generate checksums (if release-artifacts dir exists)
  const releaseDir = path.join(ROOT, "release-artifacts");
  if (fs.existsSync(releaseDir)) {
    log("Generating checksums...");
    run("sh", ["-c", "sha256sum * > SHA256SUMS.txt"], { cwd: releaseDir });
  }

  // 4. Generate release notes to temp file
  log("Generating release notes...");
  // Capture release-notes output to a file
  const notesPath = path.join(ROOT, ".release-notes.md");
  const notesContent = captureReleaseNotes(version);
  writeText(notesPath, notesContent);

  // 5. Create or update GitHub release
  log("Creating GitHub release...");
  const releaseExists = run("gh", ["release", "view", `v${version}`, "--repo", REPO], {
    allowFailure: true,
    silent: true,
  });

  const releaseFiles = fs.existsSync(releaseDir)
    ? fs.readdirSync(releaseDir).map((f) => path.join(releaseDir, f))
    : [];

  if (releaseExists) {
    run("gh", [
      "release", "edit", `v${version}`,
      "--repo", REPO,
      "--title", `Uira v${version}`,
      "--notes-file", notesPath,
    ]);
    log(`Updated existing release v${version}`);
  } else {
    const args = [
      "release", "create", `v${version}`,
      "--repo", REPO,
      "--title", `Uira v${version}`,
      "--notes-file", notesPath,
    ];
    if (version.includes("-")) args.push("--prerelease");
    args.push(...releaseFiles);
    run("gh", args);
    log(`Created release v${version}`);
  }

  // Cleanup
  fs.rmSync(notesPath, { force: true });

  console.log(`\nRelease v${version} finalized.`);
}

/** Same as releaseNotes but returns the string instead of printing. */
function captureReleaseNotes(version) {
  const prevTag = run("git", ["describe", "--tags", "--abbrev=0", "HEAD^"], {
    allowFailure: true,
    silent: true,
  });

  const commits = getCommitsSince(prevTag);
  const entries = [];
  const seenPRs = new Set();

  for (const commit of commits) {
    const pr = findPR(commit.hash);
    if (pr?.number && seenPRs.has(pr.number)) continue;
    if (pr?.number) seenPRs.add(pr.number);
    entries.push({
      hash: commit.hash,
      message: pr?.title || commit.message,
      author: pr ? `@${pr.authorLogin}` : resolveAuthor(commit.email, commit.name),
      prNumber: pr?.number,
    });
  }

  const lines = [
    `## Uira v${version}`,
    "",
    "### Installation",
    "",
    "```bash",
    "# npm",
    `npx @uiradev/uira@${version} --version`,
    `npm install -g @uiradev/uira@${version}`,
    "",
    "# cargo",
    `cargo install uira-cli@${version}            # uira-agent (main CLI)`,
    `cargo install uira-commit-hook-cli@${version} # git hooks CLI`,
    `cargo install uira-mcp-server@${version}      # MCP server`,
    "```",
    "",
    "### Changes",
    "",
  ];

  if (entries.length === 0) {
    lines.push("* Initial release");
  } else {
    for (const e of entries.reverse()) {
      const prLink = e.prNumber ? ` in https://github.com/${REPO}/pull/${e.prNumber}` : "";
      const hashRef = e.prNumber ? "" : ` (${e.hash.slice(0, 7)})`;
      lines.push(`* ${e.message} by ${e.author}${prLink}${hashRef}`);
    }
  }

  lines.push(
    "",
    "### Binaries",
    "",
    "| Platform | uira-agent | uira-commit-hook-cli | uira-mcp |",
    "|----------|------------|---------------------|----------|",
    "| macOS ARM64 | `uira-agent-darwin-arm64` | `uira-commit-hook-cli-darwin-arm64` | `uira-mcp-darwin-arm64` |",
    "| macOS x64 | `uira-agent-darwin-x64` | `uira-commit-hook-cli-darwin-x64` | `uira-mcp-darwin-x64` |",
    "| Linux x64 | `uira-agent-linux-x64-gnu` | `uira-commit-hook-cli-linux-x64-gnu` | `uira-mcp-linux-x64-gnu` |",
    "| Linux ARM64 | `uira-agent-linux-arm64-gnu` | `uira-commit-hook-cli-linux-arm64-gnu` | `uira-mcp-linux-arm64-gnu` |",
    "| Linux x64 (musl) | `uira-agent-linux-x64-musl` | `uira-commit-hook-cli-linux-x64-musl` | `uira-mcp-linux-x64-musl` |",
    "| Linux ARM64 (musl) | `uira-agent-linux-arm64-musl` | `uira-commit-hook-cli-linux-arm64-musl` | `uira-mcp-linux-arm64-musl` |",
    "| Windows x64 | `uira-agent-win32-x64-msvc.exe` | `uira-commit-hook-cli-win32-x64-msvc.exe` | `uira-mcp-win32-x64-msvc.exe` |",
  );

  if (prevTag) {
    lines.push("", `**Full Changelog**: https://github.com/${REPO}/compare/${prevTag}...v${version}`);
  }

  return lines.join("\n");
}

// ── CLI dispatcher ───────────────────────────────────────────────────────────

const commands = {
  bump,
  verify: () => verify(),
  "publish-crates": publishCrates,
  "generate-npm": generateNpm,
  "publish-npm-platforms": publishNpmPlatforms,
  "publish-npm-main": publishNpmMain,
  "publish-npm-hook": publishNpmHook,
  "release-notes": releaseNotes,
  finalize,
};

function main() {
  const [cmd, ...args] = process.argv.slice(2);

  if (!cmd || cmd === "--help" || cmd === "-h") {
    console.log("Uira Release Deployer\n");
    console.log("Usage: node scripts/release.mjs <command> [args]\n");
    console.log("Commands:");
    console.log("  bump <patch|minor|major|X.Y.Z>     Bump versions (no commit)");
    console.log("  verify                              Verify all versions match");
    console.log("  publish-crates                      Publish to crates.io");
    console.log("  generate-npm <artifacts> <output>   Generate platform npm packages");
    console.log("  publish-npm-platforms <dir>         Publish platform packages");
    console.log("  publish-npm-main                    Publish @uiradev/uira");
    console.log("  publish-npm-hook                    Publish @uiradev/hook");
    console.log("  release-notes <version>             Generate release notes");
    console.log("  finalize <version>                  Commit, tag, GitHub release");
    process.exit(0);
  }

  const handler = commands[cmd];
  if (!handler) die(`Unknown command: ${cmd}. Run with --help for usage.`);

  handler(...args);
}

main();
