#!/usr/bin/env bun
/**
 * Astrape PreToolUse Hook
 * - Blocks Task tool for agents requiring custom model routing
 * - README injection, tool validation via native Rust
 */

import { readFileSync } from 'fs';
import { dirname, join } from 'path';

const __dirname = dirname(new URL(import.meta.url).pathname);

// Agents that require custom model routing via spawn_agent MCP tool
// These agents have non-Anthropic models configured in astrape.yml
const AGENTS_REQUIRING_CUSTOM_ROUTING = new Set([
  'astrape:librarian',
  'astrape:explore',
]);

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

// Block Task tool for agents requiring custom model routing
if (toolName === 'Task') {
  const subagentType = toolInput.subagent_type || toolInput.subagentType || '';
  if (AGENTS_REQUIRING_CUSTOM_ROUTING.has(subagentType)) {
    const agentName = subagentType.replace('astrape:', '');
    console.log(JSON.stringify({
      continue: false,
      decision: 'block',
      reason: `Agent '${agentName}' requires custom model routing. Use the spawn_agent MCP tool instead:\n\n` +
        `mcp__plugin_astrape_astrape-tools__spawn_agent(agent="${agentName}", prompt="your prompt here")\n\n` +
        `This ensures the agent routes through astrape-proxy to the correct model (e.g., opencode/big-pickle for librarian).`
    }));
    process.exit(0);
  }
}

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
