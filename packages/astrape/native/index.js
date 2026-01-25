import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);

let _native;
function getNative() {
  if (!_native) {
    _native = require('./index.cjs');
  }
  return _native;
}

export function analyzeComplexity(...args) { return getNative().analyzeComplexity(...args); }
export function checkGoal(...args) { return getNative().checkGoal(...args); }
export function checkGoals(...args) { return getNative().checkGoals(...args); }
export function checkGoalsFromConfig(...args) { return getNative().checkGoalsFromConfig(...args); }
export function checkNotifications(...args) { return getNative().checkNotifications(...args); }
export function createHookOutputDeny(...args) { return getNative().createHookOutputDeny(...args); }
export function createHookOutputStop(...args) { return getNative().createHookOutputStop(...args); }
export function createHookOutputWithMessage(...args) { return getNative().createHookOutputWithMessage(...args); }
export function detectAllKeywords(...args) { return getNative().detectAllKeywords(...args); }
export function detectKeywords(...args) { return getNative().detectKeywords(...args); }
export function executeHook(...args) { return getNative().executeHook(...args); }
export function getAgent(...args) { return getNative().getAgent(...args); }
export function getHookCount(...args) { return getNative().getHookCount(...args); }
export function getSkill(...args) { return getNative().getSkill(...args); }
export function getSkillDefinition(...args) { return getNative().getSkillDefinition(...args); }
export function listAgentNames(...args) { return getNative().listAgentNames(...args); }
export function listAgents(...args) { return getNative().listAgents(...args); }
export function listGoalsFromConfig(...args) { return getNative().listGoalsFromConfig(...args); }
export function listHooks(...args) { return getNative().listHooks(...args); }
export function listSkills(...args) { return getNative().listSkills(...args); }
export function notifyBackgroundEvent(...args) { return getNative().notifyBackgroundEvent(...args); }
export function registerBackgroundTask(...args) { return getNative().registerBackgroundTask(...args); }
export function routeTaskPrompt(...args) { return getNative().routeTaskPrompt(...args); }
export function routeTaskWithAgent(...args) { return getNative().routeTaskWithAgent(...args); }
