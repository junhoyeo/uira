import {
  listAgents as nativeListAgents,
  getAgent as nativeGetAgent,
  listAgentNames as nativeListAgentNames,
  routeTaskPrompt as nativeRouteTaskPrompt,
  routeTaskWithAgent as nativeRouteTaskWithAgent,
  analyzeComplexity as nativeAnalyzeComplexity,
  getSkill as nativeGetSkill,
  getSkillDefinition as nativeGetSkillDefinition,
  listSkills as nativeListSkills,
  checkNotifications as nativeCheckNotifications,
  notifyBackgroundEvent as nativeNotifyBackgroundEvent,
  registerBackgroundTask as nativeRegisterBackgroundTask,
  type JsAgentDefinition,
  type JsRoutingResult,
  type JsComplexityAnalysis,
  type JsSkillDefinition,
  type JsNotificationResult,
} from '../index';

export type {
  JsAgentDefinition as AgentDefinition,
  JsRoutingResult as RoutingResult,
  JsComplexityAnalysis as ComplexityAnalysis,
  JsSkillDefinition as SkillDefinition,
  JsNotificationResult as NotificationResult,
};

export function listAgents(): JsAgentDefinition[] {
  return nativeListAgents();
}

export function getAgent(name: string): JsAgentDefinition | null {
  return nativeGetAgent(name);
}

export function listAgentNames(): string[] {
  return nativeListAgentNames();
}

export function routeTask(prompt: string, agentType?: string): JsRoutingResult {
  if (agentType) {
    return nativeRouteTaskWithAgent(prompt, agentType);
  }
  return nativeRouteTaskPrompt(prompt);
}

export function analyzeComplexity(prompt: string, agentType?: string): JsComplexityAnalysis {
  return nativeAnalyzeComplexity(prompt, agentType);
}

export function getSkill(name: string): string | null {
  return nativeGetSkill(name);
}

export function getSkillDefinition(name: string): JsSkillDefinition | null {
  return nativeGetSkillDefinition(name);
}

export function listSkills(): string[] {
  return nativeListSkills();
}

export function checkNotifications(sessionId: string): JsNotificationResult {
  return nativeCheckNotifications(sessionId);
}

export function notifyBackgroundEvent(event: object): void {
  nativeNotifyBackgroundEvent(JSON.stringify(event));
}

export function registerBackgroundTask(
  taskId: string,
  sessionId: string,
  parentSessionId: string,
  description: string,
  agent: string
): void {
  nativeRegisterBackgroundTask(taskId, sessionId, parentSessionId, description, agent);
}
