#!/usr/bin/env node
/**
 * Astrape PreToolUse Hook
 * README injection, tool validation, etc.
 */

import { createRequire } from 'module';
import { readFileSync } from 'fs';

const require = createRequire(import.meta.url);
const ASTRAPE_NAPI_PATH = process.env.ASTRAPE_NAPI_PATH;

let astrape;
try {
  astrape = require(`${ASTRAPE_NAPI_PATH}/index.js`);
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

// Use Astrape's hook system for pre-tool-use
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
    // Fallback: continue on error
    console.log(JSON.stringify({ continue: true }));
  }
}

main();
