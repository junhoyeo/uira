#!/usr/bin/env node
/**
 * Astrape UserPromptSubmit Hook
 * Keyword detection using native Rust bindings
 */

import { createRequire } from 'module';
import { readFileSync } from 'fs';

const require = createRequire(import.meta.url);
const ASTRAPE_NAPI_PATH = process.env.ASTRAPE_NAPI_PATH;

let astrape;
try {
  astrape = require(`${ASTRAPE_NAPI_PATH}/index.js`);
} catch (e) {
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

// Detect keywords using Astrape
const result = astrape.detectKeywords(prompt, null);

if (result) {
  console.log(JSON.stringify(result));
} else {
  console.log(JSON.stringify({ continue: true }));
}
