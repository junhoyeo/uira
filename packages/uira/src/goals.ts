import {
  checkGoal as nativeCheckGoal,
  checkGoals as nativeCheckGoals,
  checkGoalsFromConfig as nativeCheckGoalsFromConfig,
  listGoalsFromConfig as nativeListGoalsFromConfig,
  type JsGoalConfig,
  type JsGoalCheckResult,
  type JsVerificationResult,
} from '../index';

export type {
  JsGoalConfig as GoalConfig,
  JsGoalCheckResult as GoalCheckResult,
  JsVerificationResult as VerificationResult,
};

export async function checkGoal(
  directory: string,
  goal: JsGoalConfig
): Promise<JsGoalCheckResult> {
  return nativeCheckGoal(directory, goal);
}

export async function checkGoals(
  directory: string,
  goals: JsGoalConfig[]
): Promise<JsVerificationResult> {
  return nativeCheckGoals(directory, goals);
}

export async function checkGoalsFromConfig(
  directory: string
): Promise<JsVerificationResult | null> {
  return nativeCheckGoalsFromConfig(directory);
}

export function listGoalsFromConfig(directory: string): JsGoalConfig[] {
  return nativeListGoalsFromConfig(directory);
}
