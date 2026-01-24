#!/usr/bin/env bun
/**
 * Astrape UserPromptSubmit Hook
 * Uses native Rust bindings for keyword detection
 */

import { readFileSync } from 'fs';
import { dirname, join } from 'path';

const __dirname = dirname(new URL(import.meta.url).pathname);

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

// Extract prompt
const prompt = data.prompt ||
  data.message?.content ||
  (data.parts?.filter(p => p.type === 'text').map(p => p.text).join(' ')) ||
  '';

if (!prompt) {
  console.log(JSON.stringify({ continue: true }));
  process.exit(0);
}

// Detect keywords using native Rust module
const result = astrape.detectKeywords(prompt, null);

// Check for background task notifications
const sessionId = data.session_id || data.sessionId || 'default';
try {
  const notifications = astrape.checkNotifications(sessionId);
  if (notifications && notifications.hasNotifications && notifications.message) {
    // If we also have keyword result, combine messages
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
