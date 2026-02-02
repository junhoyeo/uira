import {
  executeHook as nativeExecuteHook,
  listHooks as nativeListHooks,
  getHookCount as nativeGetHookCount,
  detectKeywords as nativeDetectKeywords,
  detectAllKeywords as nativeDetectAllKeywords,
  createHookOutputWithMessage,
  createHookOutputDeny,
  createHookOutputStop,
  type JsHookInput,
  type JsHookOutput,
  type DetectedKeyword,
} from '../index';
import type { HookEvent } from './types';

export type { JsHookInput as HookInput, JsHookOutput, DetectedKeyword };

export async function executeHook(event: HookEvent, input: JsHookInput): Promise<JsHookOutput> {
  return nativeExecuteHook(event, input);
}

export function listHooks(): string[] {
  return nativeListHooks();
}

export function getHookCount(): number {
  return nativeGetHookCount();
}

export function detectKeywords(prompt: string, agent?: string): JsHookOutput | null {
  return nativeDetectKeywords(prompt, agent);
}

export function detectAllKeywords(prompt: string, agent?: string): DetectedKeyword[] {
  return nativeDetectAllKeywords(prompt, agent);
}

export const HookOutputFactory = {
  withMessage: createHookOutputWithMessage,
  deny: createHookOutputDeny,
  stop: createHookOutputStop,
};
