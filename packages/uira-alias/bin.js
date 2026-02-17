#!/usr/bin/env node
/**
 * Alias package for @uiradev/uira.
 * Re-exports the binary from the scoped package.
 */
const path = require('path');
const { execFileSync } = require('child_process');

const realBin = path.resolve(
  path.dirname(require.resolve('@uiradev/uira/package.json')),
  'bin',
  'uira.js'
);

try {
  execFileSync(process.execPath, [realBin, ...process.argv.slice(2)], {
    stdio: 'inherit',
  });
} catch (e) {
  if (e.status !== undefined) {
    process.exit(e.status);
  }
  throw e;
}
