#!/usr/bin/env node
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { createRequire } from 'module';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const require = createRequire(import.meta.url);

let uira;
try {
  uira = require(join(__dirname, '..', 'native', 'index.cjs'));
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

const prompt = data.prompt ||
  data.message?.content ||
  (data.parts?.filter(p => p.type === 'text').map(p => p.text).join(' ')) ||
  '';

if (!prompt) {
  console.log(JSON.stringify({ continue: true }));
  process.exit(0);
}

function activateRalph(sessionId, promptText) {
  const uiraDir = join(process.cwd(), '.uira');
  try {
    if (!existsSync(uiraDir)) {
      mkdirSync(uiraDir, { recursive: true });
    }
    const state = {
      active: true,
      iteration: 0,
      max_iterations: 10,
      completion_promise: "TASK COMPLETE",
      session_id: sessionId,
      prompt: promptText,
      started_at: new Date().toISOString(),
      last_checked_at: new Date().toISOString(),
      min_confidence: 50,
      require_dual_condition: true,
      session_hours: 24,
    };
    writeFileSync(join(uiraDir, 'ralph-state.json'), JSON.stringify(state, null, 2));
    return true;
  } catch {
    return false;
  }
}

const result = uira.detectKeywords(prompt, null);

if (result && result.message) {
  const isRalph = result.message.includes('ralph-mode');
  if (isRalph) {
    const sessionId = data.session_id || data.sessionId || 'default';
    const activated = activateRalph(sessionId, prompt);
    if (!activated) {
      console.error('[uira] Warning: Failed to activate ralph mode - state file write failed');
    }
  }
}

const sessionId = data.session_id || data.sessionId || 'default';
try {
  const notifications = uira.checkNotifications(sessionId);
  if (notifications && notifications.hasNotifications && notifications.message) {
    if (result) {
      result.message = (notifications.message + '\n\n' + (result.message || '')).trim();
      console.log(JSON.stringify(result));
    } else {
      console.log(JSON.stringify({
        continue: true,
        message: notifications.message
      }));
    }
    process.exit(0);
  }
} catch (e) {
  console.error('[uira] checkNotifications error:', e.message);
}

if (result) {
  console.log(JSON.stringify(result));
} else {
  console.log(JSON.stringify({ continue: true }));
}
