export * from './types';

export {
  executeHook,
  listHooks,
  getHookCount,
  detectKeywords,
  detectAllKeywords,
  HookOutputFactory,
  type HookInput,
  type JsHookOutput,
  type DetectedKeyword,
} from './hooks';

export {
  listAgents,
  getAgent,
  listAgentNames,
  routeTask,
  analyzeComplexity,
  getSkill,
  getSkillDefinition,
  listSkills,
  checkNotifications,
  notifyBackgroundEvent,
  registerBackgroundTask,
  type AgentDefinition,
  type RoutingResult,
  type ComplexityAnalysis,
  type SkillDefinition,
  type NotificationResult,
} from './agent';

export {
  checkGoal,
  checkGoals,
  checkGoalsFromConfig,
  listGoalsFromConfig,
  type GoalConfig,
  type GoalCheckResult,
  type VerificationResult,
} from './goals';

export { Linter, type LintDiagnostic, type LintRule, type LinterConfig } from './oxc';

export { CommentChecker, type CommentInfo, type CommentCheckResult } from './comments';
