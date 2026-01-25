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
} catch (e) {
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

if (data.user_requested || data.userRequested) {
  console.log(JSON.stringify({ continue: true }));
  process.exit(0);
}

const hookInput = {
  sessionId: data.session_id || data.sessionId,
  prompt: data.prompt,
  directory: process.cwd(),
  stopReason: data.stop_reason || data.stopReason,
  userRequested: data.user_requested || data.userRequested,
  transcriptPath: data.transcript_path || data.transcriptPath,
};

try {
  const result = await astrape.executeHook('stop', hookInput);
  console.log(JSON.stringify({
    continue: result.continue,
    message: result.message,
    stopReason: result.stopReason,
    decision: result.decision,
    reason: result.reason,
  }));
} catch (e) {
  console.log(JSON.stringify({ continue: true }));
}

process.exit(0);
