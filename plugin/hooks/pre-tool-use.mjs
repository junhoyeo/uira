#!/usr/bin/env bun
/**
 * Astrape PreToolUse Hook
 * README injection, tool validation via native Rust
 */

import { readFileSync } from 'fs';
import { dirname, join } from 'path';

const __dirname = dirname(new URL(import.meta.url).pathname);

// Load native module
let astrape;
try {
  astrape = require(join(__dirname, '..', 'native', 'index.js'));
} catch {
  console.log(JSON.stringify({ continue: true }));
  process.exit(0);
}

// Read stdin
let input = '';
try {
  input = readFileSync(0, 'utf8');
} catch {
  console.log(JSON.stringify({ continue: true }));
  process.exit(0);
}

// Parse JSON input
let data;
try {
  data = JSON.parse(input);
} catch {
  console.log(JSON.stringify({ continue: true }));
  process.exit(0);
}

const toolName = data.tool_name || data.toolName || '';
const toolInput = data.tool_input || data.toolInput || {};

// Execute pre-tool-use hooks via native module
async function main() {
  try {
    const result = await astrape.executeHook('pre-tool-use', {
      sessionId: data.session_id || data.sessionId || 'default',
      toolName: toolName,
      toolInput: JSON.stringify(toolInput),
      directory: process.cwd(),
    });

    console.log(JSON.stringify(result));
  } catch {
    console.log(JSON.stringify({ continue: true }));
  }
}

main();
