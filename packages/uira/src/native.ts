/* eslint-disable @typescript-eslint/no-var-requires */

let nativeBindings: NativeBindings | null = null;
let loadError: Error | null = null;

try {
  nativeBindings = require('../uira.node');
} catch (e) {
  loadError = e instanceof Error ? e : new Error(String(e));
}

function assertNativeLoaded<T>(fn: T | undefined, name: string): T {
  if (!nativeBindings) {
    const message = loadError
      ? `Native module failed to load: ${loadError.message}. ` +
        'Make sure you have installed the correct platform-specific package ' +
        '(@uiradev/uira-darwin-arm64, @uiradev/uira-linux-x64-gnu, etc.)'
      : `Native module not available. Function '${name}' requires the native module.`;
    throw new Error(message);
  }
  if (fn === undefined) {
    throw new Error(`Function '${name}' is not available in the native module.`);
  }
  return fn;
}

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

export interface JsAgentDefinition {
  name: string;
  description: string;
  model?: string;
  tier: string;
  prompt: string;
  tools: string[];
}

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

export interface JsSkillDefinition {
  name: string;
  description: string;
  template: string;
  agent?: string;
  model?: string;
  argumentHint?: string;
}

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

interface NativeBindings {
  executeHook(event: string, input: JsHookInput): Promise<JsHookOutput>;
  listHooks(): string[];
  getHookCount(): number;
  detectKeywords(prompt: string, agent?: string): JsHookOutput | null;
  detectAllKeywords(prompt: string, agent?: string): DetectedKeyword[];
  createHookOutputWithMessage(message: string): JsHookOutput;
  createHookOutputDeny(reason: string): JsHookOutput;
  createHookOutputStop(reason: string): JsHookOutput;

  listAgents(): JsAgentDefinition[];
  getAgent(name: string): JsAgentDefinition | null;
  listAgentNames(): string[];

  routeTaskPrompt(prompt: string): JsRoutingResult;
  routeTaskWithAgent(prompt: string, agentType?: string): JsRoutingResult;
  analyzeComplexity(prompt: string, agentType?: string): JsComplexityAnalysis;

  getSkill(name: string): string | null;
  getSkillDefinition(name: string): JsSkillDefinition | null;
  listSkills(): string[];

  checkNotifications(sessionId: string): JsNotificationResult;
  notifyBackgroundEvent(eventJson: string): void;
  registerBackgroundTask(
    taskId: string,
    sessionId: string,
    parentSessionId: string,
    description: string,
    agent: string
  ): void;

  checkGoal(directory: string, goal: JsGoalConfig): Promise<JsGoalCheckResult>;
  checkGoals(directory: string, goals: JsGoalConfig[]): Promise<JsVerificationResult>;
  checkGoalsFromConfig(directory: string): Promise<JsVerificationResult | null>;
  listGoalsFromConfig(directory: string): JsGoalConfig[];
}

export const executeHook = (event: string, input: JsHookInput) =>
  assertNativeLoaded(nativeBindings?.executeHook, 'executeHook')(event, input);

export const listHooks = () =>
  assertNativeLoaded(nativeBindings?.listHooks, 'listHooks')();

export const getHookCount = () =>
  assertNativeLoaded(nativeBindings?.getHookCount, 'getHookCount')();

export const detectKeywords = (prompt: string, agent?: string) =>
  assertNativeLoaded(nativeBindings?.detectKeywords, 'detectKeywords')(prompt, agent);

export const detectAllKeywords = (prompt: string, agent?: string) =>
  assertNativeLoaded(nativeBindings?.detectAllKeywords, 'detectAllKeywords')(prompt, agent);

export const createHookOutputWithMessage = (message: string) =>
  assertNativeLoaded(nativeBindings?.createHookOutputWithMessage, 'createHookOutputWithMessage')(message);

export const createHookOutputDeny = (reason: string) =>
  assertNativeLoaded(nativeBindings?.createHookOutputDeny, 'createHookOutputDeny')(reason);

export const createHookOutputStop = (reason: string) =>
  assertNativeLoaded(nativeBindings?.createHookOutputStop, 'createHookOutputStop')(reason);

export const listAgents = () =>
  assertNativeLoaded(nativeBindings?.listAgents, 'listAgents')();

export const getAgent = (name: string) =>
  assertNativeLoaded(nativeBindings?.getAgent, 'getAgent')(name);

export const listAgentNames = () =>
  assertNativeLoaded(nativeBindings?.listAgentNames, 'listAgentNames')();

export const routeTaskPrompt = (prompt: string) =>
  assertNativeLoaded(nativeBindings?.routeTaskPrompt, 'routeTaskPrompt')(prompt);

export const routeTaskWithAgent = (prompt: string, agentType?: string) =>
  assertNativeLoaded(nativeBindings?.routeTaskWithAgent, 'routeTaskWithAgent')(prompt, agentType);

export const analyzeComplexity = (prompt: string, agentType?: string) =>
  assertNativeLoaded(nativeBindings?.analyzeComplexity, 'analyzeComplexity')(prompt, agentType);

export const getSkill = (name: string) =>
  assertNativeLoaded(nativeBindings?.getSkill, 'getSkill')(name);

export const getSkillDefinition = (name: string) =>
  assertNativeLoaded(nativeBindings?.getSkillDefinition, 'getSkillDefinition')(name);

export const listSkills = () =>
  assertNativeLoaded(nativeBindings?.listSkills, 'listSkills')();

export const checkNotifications = (sessionId: string) =>
  assertNativeLoaded(nativeBindings?.checkNotifications, 'checkNotifications')(sessionId);

export const notifyBackgroundEvent = (eventJson: string) =>
  assertNativeLoaded(nativeBindings?.notifyBackgroundEvent, 'notifyBackgroundEvent')(eventJson);

export const registerBackgroundTask = (
  taskId: string,
  sessionId: string,
  parentSessionId: string,
  description: string,
  agent: string
) =>
  assertNativeLoaded(nativeBindings?.registerBackgroundTask, 'registerBackgroundTask')(
    taskId,
    sessionId,
    parentSessionId,
    description,
    agent
  );

export const checkGoal = (directory: string, goal: JsGoalConfig) =>
  assertNativeLoaded(nativeBindings?.checkGoal, 'checkGoal')(directory, goal);

export const checkGoals = (directory: string, goals: JsGoalConfig[]) =>
  assertNativeLoaded(nativeBindings?.checkGoals, 'checkGoals')(directory, goals);

export const checkGoalsFromConfig = (directory: string) =>
  assertNativeLoaded(nativeBindings?.checkGoalsFromConfig, 'checkGoalsFromConfig')(directory);

export const listGoalsFromConfig = (directory: string) =>
  assertNativeLoaded(nativeBindings?.listGoalsFromConfig, 'listGoalsFromConfig')(directory);
