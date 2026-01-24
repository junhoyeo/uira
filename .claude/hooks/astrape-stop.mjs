#!/usr/bin/env node
/**
 * Astrape Stop Hook
 * Controls continuation behavior when Claude stops
 */

import { readFileSync } from 'fs';

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

// Check stop reason
const stopReason = data.stop_reason || data.stopReason || '';

// If user requested stop, allow it
if (data.user_requested || data.userRequested) {
  console.log(JSON.stringify({ continue: true }));
  process.exit(0);
}

// Check for active ralph/ultrawork modes that should continue
// TODO: Read from .omc/state files when implemented
const activeStates = [];
try {
  // Check for ultrawork state
  const ultraworkState = JSON.parse(
    readFileSync('.omc/ultrawork-state.json', 'utf8')
  );
  if (ultraworkState.active) {
    activeStates.push('ultrawork');
  }
} catch {
  // No active state
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
