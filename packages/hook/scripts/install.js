#!/usr/bin/env node

// Postinstall script for @uiradev/hook
// This script runs after npm install and sets up git hooks automatically

const fs = require('fs');
const path = require('path');

function findGitDir() {
  let dir = process.cwd();
  while (dir !== path.dirname(dir)) {
    const gitDir = path.join(dir, '.git');
    if (fs.existsSync(gitDir)) {
      return gitDir;
    }
    dir = path.dirname(dir);
  }
  return null;
}

function main() {
  // Skip if running in CI
  if (process.env.CI) {
    return;
  }

  // Skip if UIRA_SKIP_INSTALL is set
  if (process.env.UIRA_SKIP_INSTALL) {
    return;
  }

  const gitDir = findGitDir();
  if (!gitDir) {
    // Not a git repository, skip silently
    return;
  }

  console.log('uira-hook: Run `uira-hook install` to set up git hooks');
}

main();
