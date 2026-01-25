#!/usr/bin/env bun
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { dirname, join } from 'path';

const __dirname = dirname(new URL(import.meta.url).pathname);

let astrape;
try {
  astrape = require(join(__dirname, '..', 'native', 'index.js'));
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
  const astrapeDir = join(process.cwd(), '.astrape');
  try {
    if (!existsSync(astrapeDir)) {
      mkdirSync(astrapeDir, { recursive: true });
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
    writeFileSync(join(astrapeDir, 'ralph-state.json'), JSON.stringify(state, null, 2));
    return true;
  } catch {
    return false;
  }
}

const result = astrape.detectKeywords(prompt, null);

if (result && result.message) {
  const isRalph = result.message.includes('ralph-mode');
  if (isRalph) {
    const sessionId = data.session_id || data.sessionId || 'default';
    const activated = activateRalph(sessionId, prompt);
    if (!activated) {
      console.error('[astrape] Warning: Failed to activate ralph mode - state file write failed');
    }
  }
}

const sessionId = data.session_id || data.sessionId || 'default';
try {
  const notifications = astrape.checkNotifications(sessionId);
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
} catch {}

if (result) {
  console.log(JSON.stringify(result));
} else {
  console.log(JSON.stringify({ continue: true }));
}
