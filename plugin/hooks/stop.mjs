#!/usr/bin/env bun
/**
 * Astrape Stop Hook
 * Continuation control for persistent modes
 */

import { readFileSync, existsSync } from 'fs';
import { join } from 'path';

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

// If user requested stop, allow it
if (data.user_requested || data.userRequested) {
  console.log(JSON.stringify({ continue: true }));
  process.exit(0);
}

// Check for active modes that should continue
const activeStates = [];
const stateDir = join(process.cwd(), '.astrape', 'state');

const modes = ['ultrawork', 'ralph', 'autopilot', 'ultrapilot'];
for (const mode of modes) {
  try {
    const statePath = join(stateDir, `${mode}-state.json`);
    if (existsSync(statePath)) {
      const state = JSON.parse(readFileSync(statePath, 'utf8'));
      if (state.active) {
        activeStates.push(mode);
      }
    }
  } catch {
    // Ignore errors
  }
}

if (activeStates.length > 0) {
  console.log(JSON.stringify({
    continue: false,
    stopReason: `Stop hook prevented continuation`,
    message: `Active modes: ${activeStates.join(', ')}`
  }));
} else {
  console.log(JSON.stringify({ continue: true }));
}
