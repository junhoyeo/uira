#!/usr/bin/env bun
/**
 * Astrape PostToolUse Hook
 * - Runs comment-checker on Write/Edit/MultiEdit results
 * - Tracks background task completion events
 */

import { readFileSync, existsSync } from 'fs';
import { dirname, join } from 'path';
import { spawn } from 'bun';

const __dirname = dirname(new URL(import.meta.url).pathname);

// Load native module
let astrape;
try {
  astrape = require(join(__dirname, '..', 'native', 'index.js'));
} catch {
  // Native module not available, continue without it
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

// Track background task completion
if (toolName === 'Task' && data.tool_response && astrape?.notifyBackgroundEvent) {
  try {
    const response = typeof data.tool_response === 'string'
      ? JSON.parse(data.tool_response)
      : data.tool_response;
    if (response.agentId) {
      astrape.notifyBackgroundEvent(JSON.stringify({
        type: 'task.completed',
        properties: { taskId: response.agentId }
      }));
    }
  } catch {}
}

// Comment checker for file writing tools
const CHECKABLE_TOOLS = ['Write', 'Edit', 'MultiEdit', 'NotebookEdit'];
if (!CHECKABLE_TOOLS.includes(toolName)) {
  console.log(JSON.stringify({ continue: true }));
  process.exit(0);
}

// Find comment-checker binary
function findCommentChecker() {
  const candidates = [
    join(__dirname, '..', '..', '..', 'target', 'release', 'comment-checker'),
    join(__dirname, '..', '..', '..', 'target', 'debug', 'comment-checker'),
    // Check if in PATH (will be found by spawn)
  ];

  for (const path of candidates) {
    if (existsSync(path)) {
      return path;
    }
  }

  // Try PATH
  return 'comment-checker';
}

async function runCommentChecker() {
  const binary = findCommentChecker();

  // Prepare input for comment-checker
  const checkerInput = {
    tool_name: toolName,
    tool_input: toolInput,
    session_id: data.session_id || data.sessionId,
    cwd: process.cwd(),
  };

  try {
    const proc = spawn([binary], {
      stdin: 'pipe',
      stdout: 'pipe',
      stderr: 'pipe',
    });

    // Write input to stdin
    proc.stdin.write(JSON.stringify(checkerInput));
    proc.stdin.end();

    // Read stderr concurrently with waiting for exit to avoid deadlock
    // (child blocks if pipe buffer fills before parent reads)
    const [exitCode, stderr] = await Promise.all([
      proc.exited,
      new Response(proc.stderr).text(),
    ]);

    // Exit code 2 = comments detected
    if (exitCode === 2 && stderr.trim()) {
      return {
        continue: true,
        message: stderr.trim(),
      };
    }
  } catch (err) {
    // Binary not found or failed to execute - silently continue
    // console.error('[post-tool-use] Comment checker error:', err.message);
  }

  return { continue: true };
}

// Run async
runCommentChecker().then(result => {
  console.log(JSON.stringify(result));
}).catch(() => {
  console.log(JSON.stringify({ continue: true }));
});
