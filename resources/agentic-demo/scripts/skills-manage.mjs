#!/usr/bin/env node

import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const rootDir = path.resolve(scriptDir, "..");
const customLockPath = path.join(rootDir, ".skill-lock.json");
const nativeLockPath = path.join(rootDir, "skills-lock.json");
const projectSkillsDir = path.join(rootDir, ".agents", "skills");
const command = process.argv[2] ?? "help";
const force = process.argv.includes("--force");
const sourceCheckoutCache = new Map();
const tempCheckouts = [];

function fail(message) {
  console.error(message);
  process.exit(1);
}

function run(bin, args, options = {}) {
  const result = spawnSync(bin, args, {
    cwd: rootDir,
    stdio: "inherit",
    ...options
  });

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function runText(bin, args) {
  const result = spawnSync(bin, args, {
    cwd: rootDir,
    encoding: "utf8"
  });

  if (result.status !== 0) {
    return "";
  }

  return result.stdout.trim();
}

function cloneRepo(url) {
  if (sourceCheckoutCache.has(url)) {
    return sourceCheckoutCache.get(url);
  }

  const checkoutDir = mkdtempSync(path.join(os.tmpdir(), "skills-manage-"));
  const result = spawnSync("git", ["clone", "--depth", "1", url, checkoutDir], {
    cwd: rootDir,
    encoding: "utf8",
    stdio: "pipe"
  });

  if (result.status !== 0) {
    rmSync(checkoutDir, { recursive: true, force: true });
    sourceCheckoutCache.set(url, "");
    return "";
  }

  sourceCheckoutCache.set(url, checkoutDir);
  tempCheckouts.push(checkoutDir);
  return checkoutDir;
}

function getSyncSourceDir(meta, installedDir) {
  const sourceSkillPath = meta.sourceSkillPath;
  if (!sourceSkillPath || !meta.sourceUrl) {
    return installedDir;
  }

  const checkoutDir = cloneRepo(meta.sourceUrl);
  if (!checkoutDir) {
    return installedDir;
  }

  const sourceDir = path.join(checkoutDir, path.dirname(sourceSkillPath));
  if (!existsSync(sourceDir)) {
    return installedDir;
  }

  return sourceDir;
}

function readJson(filePath) {
  return JSON.parse(readFileSync(filePath, "utf8"));
}

function writeJson(filePath, value) {
  writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function loadCustomLock() {
  if (!existsSync(customLockPath)) {
    fail("Missing .skill-lock.json");
  }

  return readJson(customLockPath);
}

function buildNativeLock(customLock) {
  const skills = {};

  for (const [name, meta] of Object.entries(customLock.skills ?? {})) {
    skills[name] = {
      source: meta.source,
      sourceType: meta.sourceType ?? "github"
    };
  }

  return {
    version: 1,
    skills
  };
}

function writeNativeLock() {
  const customLock = loadCustomLock();
  const nativeLock = buildNativeLock(customLock);
  writeJson(nativeLockPath, nativeLock);
  console.log(`Wrote ${path.relative(rootDir, nativeLockPath)}`);
}

function hasProjectInstalls() {
  return existsSync(projectSkillsDir) && readdirSync(projectSkillsDir).length > 0;
}

function ensureProjectInstalls() {
  writeNativeLock();

  if (!hasProjectInstalls()) {
    restoreProjectInstalls();
  }
}

function restoreProjectInstalls() {
  console.log("Installing project skills from skills-lock.json");
  run("npx", ["skills", "experimental_install"]);
}

function hasGitChanges(targetPath) {
  const relativePath = path.relative(rootDir, targetPath);
  const output = runText("git", ["status", "--porcelain", "--", relativePath]);
  return output.length > 0;
}

function syncInstalledSkills({ updateCustomLock = false }) {
  const customLock = loadCustomLock();
  const skipped = [];
  const synced = [];
  const now = new Date().toISOString();

  mkdirSync(projectSkillsDir, { recursive: true });

  for (const [name, meta] of Object.entries(customLock.skills ?? {})) {
    const installedDir = path.join(projectSkillsDir, name);
    const targetDir = path.join(rootDir, path.dirname(meta.skillPath));
    const sourceDir = getSyncSourceDir(meta, installedDir);

    if (!existsSync(installedDir)) {
      skipped.push(`${name} (missing install at ${path.relative(rootDir, installedDir)})`);
      continue;
    }

    if (!existsSync(sourceDir)) {
      skipped.push(`${name} (missing sync source at ${sourceDir})`);
      continue;
    }

    if (!force && existsSync(targetDir) && hasGitChanges(targetDir)) {
      skipped.push(`${name} (target has local git changes)`);
      continue;
    }

    rmSync(targetDir, { recursive: true, force: true });
    mkdirSync(path.dirname(targetDir), { recursive: true });
    cpSync(sourceDir, targetDir, {
      recursive: true,
      filter: (src) => path.basename(src) !== ".git"
    });
    synced.push(`${name} -> ${path.relative(rootDir, targetDir)}`);

    if (updateCustomLock) {
      meta.updatedAt = now;
    }
  }

  if (updateCustomLock) {
    writeJson(customLockPath, customLock);
  }

  if (synced.length > 0) {
    console.log("Synced skills:");
    for (const item of synced) {
      console.log(`- ${item}`);
    }
  }

  if (skipped.length > 0) {
    console.log("Skipped skills:");
    for (const item of skipped) {
      console.log(`- ${item}`);
    }

    if (!force) {
      console.log("Re-run with --force to overwrite managed target folders.");
    }
  }
}

process.on("exit", () => {
  for (const checkoutDir of tempCheckouts) {
    rmSync(checkoutDir, { recursive: true, force: true });
  }
});

function printInstallCoverage() {
  const customLock = loadCustomLock();
  const expectedNames = Object.keys(customLock.skills ?? {});
  const installedNames = existsSync(projectSkillsDir) ? readdirSync(projectSkillsDir).sort() : [];
  const installedSet = new Set(installedNames);
  const missing = expectedNames.filter((name) => !installedSet.has(name));

  console.log(`Installed ${installedNames.length} managed skill folder(s) in .agents/skills`);

  if (missing.length > 0) {
    console.log("Missing from project installs:");
    for (const name of missing) {
      console.log(`- ${name}`);
    }
  }
}

switch (command) {
  case "lock":
    writeNativeLock();
    break;
  case "install":
    writeNativeLock();
    restoreProjectInstalls();
    syncInstalledSkills({ updateCustomLock: false });
    printInstallCoverage();
    break;
  case "sync":
    ensureProjectInstalls();
    syncInstalledSkills({ updateCustomLock: false });
    printInstallCoverage();
    break;
  case "check":
    ensureProjectInstalls();
    printInstallCoverage();
    break;
  case "update":
    writeNativeLock();
    restoreProjectInstalls();
    syncInstalledSkills({ updateCustomLock: true });
    printInstallCoverage();
    break;
  default:
    console.log("Usage: node scripts/skills-manage.mjs <lock|install|sync|check|update> [--force]");
    process.exit(command === "help" ? 0 : 1);
}
