#!/usr/bin/env node
import { readFileSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { createRequire } from 'module';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const require = createRequire(import.meta.url);

const AGENTS_REQUIRING_CUSTOM_ROUTING = new Set([
  'astrape:librarian',
  'astrape:explore',
]);

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
