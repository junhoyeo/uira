#!/usr/bin/env bun
/**
 * Astrape PostToolUse Hook
 * - Runs all PostToolUse hooks via native executeHook (includes CommentCheckerHook)
 * - Tracks background task completion events
 */

import { readFileSync } from 'fs';
import { dirname, join } from 'path';

const __dirname = dirname(new URL(import.meta.url).pathname);

// Load native module
let astrape;
try {
  astrape = require(join(__dirname, '..', 'native', 'index.js'));
} catch {
  // Native module not available, continue without it
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

// Execute all PostToolUse hooks via native module (includes CommentCheckerHook)
async function runHooks() {
  try {
    const hookInput = {
      sessionId: data.session_id || data.sessionId,
      toolName: toolName,
      toolInput: JSON.stringify(toolInput),
      toolOutput: data.tool_output ? JSON.stringify(data.tool_output) : undefined,
      directory: process.cwd(),
    };

    const result = await astrape.executeHook('post-tool-use', hookInput);
    
    if (result) {
      console.log(JSON.stringify({
        continue: result.shouldContinue !== false,
        message: result.message || undefined,
        reason: result.reason || undefined,
      }));
    } else {
      console.log(JSON.stringify({ continue: true }));
    }
  } catch (err) {
    // Hook execution failed - silently continue
    // console.error('[post-tool-use] Hook execution error:', err.message);
    console.log(JSON.stringify({ continue: true }));
  }
}

runHooks();
