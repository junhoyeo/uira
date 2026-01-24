#!/usr/bin/env node
/**
 * Astrape SessionStart Hook
 * Initialize session, load state, etc.
 */

import { readFileSync, existsSync, mkdirSync, writeFileSync } from 'fs';
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

const sessionId = data.session_id || data.sessionId || `session-${Date.now()}`;
const stateDir = join(process.cwd(), '.astrape', 'state');

// Ensure state directory exists
try {
  if (!existsSync(stateDir)) {
    mkdirSync(stateDir, { recursive: true });
  }

  // Initialize session state
  const sessionState = {
    sessionId,
    startedAt: new Date().toISOString(),
    cwd: process.cwd(),
  };

  writeFileSync(
    join(stateDir, 'session.json'),
    JSON.stringify(sessionState, null, 2)
  );
} catch {
  // Non-fatal, continue
}

// Return success with optional message
console.log(JSON.stringify({
  continue: true,
  message: 'Success'
}));
