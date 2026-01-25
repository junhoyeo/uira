#!/usr/bin/env node
import { readFileSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { createRequire } from 'module';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const require = createRequire(import.meta.url);

let astrape;
try {
  astrape = require(join(__dirname, '..', 'native', 'index.cjs'));
} catch {
  console.log(JSON.stringify({ continue: true }));
  process.exit(0);
}

let input = '';
try {
  input = readFileSync(0, 'utf8');
} catch {
  console.log(JSON.stringify({ continue: true }));
  process.exit(0);
}

let data;
try {
  data = JSON.parse(input);
} catch {
  console.log(JSON.stringify({ continue: true }));
  process.exit(0);
}

const toolName = data.tool_name || data.toolName || '';
const toolInput = data.tool_input || data.toolInput || {};

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
  } catch {
    console.log(JSON.stringify({ continue: true }));
  }
}

runHooks();
