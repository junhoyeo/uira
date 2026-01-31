/* eslint-disable @typescript-eslint/no-var-requires */

/**
 * Native bindings loaded from the Rust napi module.
 * The .node file is built by napi-rs during `npm run build`.
 */

// Try to load the native module
let nativeBindings: NativeBindings;

try {
  nativeBindings = require('../uira.node');
} catch {
  // During development or when native module isn't built yet
  nativeBindings = {} as NativeBindings;
}

// ============================================================================
// Hook System
// ============================================================================

export interface JsHookInput {
  sessionId?: string;
  prompt?: string;
  toolName?: string;
  toolInput?: string;
  toolOutput?: string;
  directory?: string;
  stopReason?: string;
  userRequested?: boolean;
  transcriptPath?: string;
}

export interface JsHookOutput {
  continue: boolean;
  message?: string;
  stopReason?: string;
  decision?: string;
  reason?: string;
  additionalContext?: string;
  suppressOutput?: boolean;
  systemMessage?: string;
}

export interface DetectedKeyword {
  keywordType: string;
  message: string;
}

// ============================================================================
// Agent System
// ============================================================================

export interface JsAgentDefinition {
  name: string;
  description: string;
  model?: string;
  tier: string;
  prompt: string;
  tools: string[];
}

// ============================================================================
// Model Routing
// ============================================================================

export interface JsRoutingResult {
  model: string;
  tier: string;
  reasoning: string;
  confidence: number;
  escalated: boolean;
}

export interface JsComplexityAnalysis {
  tier: string;
  model: string;
  analysis: string;
}

// ============================================================================
// Skill System
// ============================================================================

export interface JsSkillDefinition {
  name: string;
  description: string;
  template: string;
  agent?: string;
  model?: string;
  argumentHint?: string;
}

// ============================================================================
// Background Tasks
// ============================================================================

export interface JsBackgroundTask {
  id: string;
  sessionId: string;
  parentSessionId: string;
  description: string;
  agent: string;
  status: string;
  result?: string;
  error?: string;
}

export interface JsNotificationResult {
  hasNotifications: boolean;
  message?: string;
  notificationCount: number;
}

// ============================================================================
// Goal Verification
// ============================================================================

export interface JsGoalCheckResult {
  name: string;
  score: number;
  target: number;
  passed: boolean;
  durationMs: number;
  error?: string;
}

export interface JsVerificationResult {
  allPassed: boolean;
  results: JsGoalCheckResult[];
  iteration: number;
}

export interface JsGoalConfig {
  name: string;
  workspace?: string;
  command: string;
  target: number;
  timeoutSecs: number;
  enabled: boolean;
  description?: string;
}

// ============================================================================
// Native Bindings Interface
// ============================================================================

interface NativeBindings {
  // Hook System
  executeHook(event: string, input: JsHookInput): Promise<JsHookOutput>;
  listHooks(): string[];
  getHookCount(): number;
  detectKeywords(prompt: string, agent?: string): JsHookOutput | null;
  detectAllKeywords(prompt: string, agent?: string): DetectedKeyword[];
  createHookOutputWithMessage(message: string): JsHookOutput;
  createHookOutputDeny(reason: string): JsHookOutput;
  createHookOutputStop(reason: string): JsHookOutput;

  // Agent System
  listAgents(): JsAgentDefinition[];
  getAgent(name: string): JsAgentDefinition | null;
  listAgentNames(): string[];

  // Model Routing
  routeTaskPrompt(prompt: string): JsRoutingResult;
  routeTaskWithAgent(prompt: string, agentType?: string): JsRoutingResult;
  analyzeComplexity(prompt: string, agentType?: string): JsComplexityAnalysis;

  // Skill System
  getSkill(name: string): string | null;
  getSkillDefinition(name: string): JsSkillDefinition | null;
  listSkills(): string[];

  // Background Tasks
  checkNotifications(sessionId: string): JsNotificationResult;
  notifyBackgroundEvent(eventJson: string): void;
  registerBackgroundTask(
    taskId: string,
    sessionId: string,
    parentSessionId: string,
    description: string,
    agent: string
  ): void;

  // Goal Verification
  checkGoal(directory: string, goal: JsGoalConfig): Promise<JsGoalCheckResult>;
  checkGoals(directory: string, goals: JsGoalConfig[]): Promise<JsVerificationResult>;
  checkGoalsFromConfig(directory: string): Promise<JsVerificationResult | null>;
  listGoalsFromConfig(directory: string): JsGoalConfig[];
}

// ============================================================================
// Exports
// ============================================================================

// Hook System
export const executeHook = nativeBindings.executeHook;
export const listHooks = nativeBindings.listHooks;
export const getHookCount = nativeBindings.getHookCount;
export const detectKeywords = nativeBindings.detectKeywords;
export const detectAllKeywords = nativeBindings.detectAllKeywords;
export const createHookOutputWithMessage = nativeBindings.createHookOutputWithMessage;
export const createHookOutputDeny = nativeBindings.createHookOutputDeny;
export const createHookOutputStop = nativeBindings.createHookOutputStop;

// Agent System
export const listAgents = nativeBindings.listAgents;
export const getAgent = nativeBindings.getAgent;
export const listAgentNames = nativeBindings.listAgentNames;

// Model Routing
export const routeTaskPrompt = nativeBindings.routeTaskPrompt;
export const routeTaskWithAgent = nativeBindings.routeTaskWithAgent;
export const analyzeComplexity = nativeBindings.analyzeComplexity;

// Skill System
export const getSkill = nativeBindings.getSkill;
export const getSkillDefinition = nativeBindings.getSkillDefinition;
export const listSkills = nativeBindings.listSkills;

// Background Tasks
export const checkNotifications = nativeBindings.checkNotifications;
export const notifyBackgroundEvent = nativeBindings.notifyBackgroundEvent;
export const registerBackgroundTask = nativeBindings.registerBackgroundTask;

// Goal Verification
export const checkGoal = nativeBindings.checkGoal;
export const checkGoals = nativeBindings.checkGoals;
export const checkGoalsFromConfig = nativeBindings.checkGoalsFromConfig;
export const listGoalsFromConfig = nativeBindings.listGoalsFromConfig;
