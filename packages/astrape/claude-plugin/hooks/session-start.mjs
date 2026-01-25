#!/usr/bin/env node
import { readFileSync, existsSync, mkdirSync, writeFileSync } from 'fs';
import { join } from 'path';

let input = '';
try {
  input = readFileSync(0, 'utf8');
} catch {
  console.log(JSON.stringify({ continue: true, message: 'Success' }));
  process.exit(0);
}

let data;
try {
  data = JSON.parse(input);
} catch {
  console.log(JSON.stringify({ continue: true, message: 'Success' }));
  process.exit(0);
}

const sessionId = data.session_id || data.sessionId || `session-${Date.now()}`;
const stateDir = join(process.cwd(), '.astrape', 'state');

try {
  if (!existsSync(stateDir)) {
    mkdirSync(stateDir, { recursive: true });
  }

  const sessionState = {
    sessionId,
    startedAt: new Date().toISOString(),
    cwd: process.cwd(),
    version: '0.1.0'
  };

  writeFileSync(
    join(stateDir, 'session.json'),
    JSON.stringify(sessionState, null, 2)
  );
} catch {}

console.log(JSON.stringify({ continue: true, message: 'Success' }));
