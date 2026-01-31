export type {
  JsHookInput,
  JsHookOutput,
  DetectedKeyword,
  JsAgentDefinition,
  JsRoutingResult,
  JsComplexityAnalysis,
  JsSkillDefinition,
  JsBackgroundTask,
  JsNotificationResult,
  JsGoalCheckResult,
  JsVerificationResult,
  JsGoalConfig,
} from './native';

export type HookEvent =
  | 'user-prompt-submit'
  | 'stop'
  | 'session-start'
  | 'pre-tool-use'
  | 'post-tool-use'
  | 'session-idle'
  | 'messages-transform';

export type ModelTier = 'LOW' | 'MEDIUM' | 'HIGH';

export type LintSeverity = 'error' | 'warning' | 'info';

export interface LintDiagnostic {
  file: string;
  line: number;
  column: number;
  message: string;
  rule: string;
  severity: LintSeverity;
  suggestion?: string;
}

export type LintRule =
  | 'no-console'
  | 'no-debugger'
  | 'no-alert'
  | 'no-eval'
  | 'no-var'
  | 'prefer-const'
  | 'no-unused-vars'
  | 'no-empty-function'
  | 'no-duplicate-keys'
  | 'no-param-reassign';

export interface LinterConfig {
  rules?: LintRule[];
}

export interface CommentInfo {
  text: string;
  lineNumber: number;
  column: number;
  file: string;
  commentType: 'line' | 'block' | 'doc';
}

export interface CommentCheckResult {
  hasComments: boolean;
  comments: CommentInfo[];
  message?: string;
}
