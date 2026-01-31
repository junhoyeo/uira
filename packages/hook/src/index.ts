export {
  executeHook,
  listHooks,
  getHookCount,
  detectKeywords,
  detectAllKeywords,
  HookOutputFactory,
  type HookInput,
  type JsHookOutput as HookOutput,
  type DetectedKeyword,
} from '@uiradev/uira/hooks';

export type { HookEvent } from '@uiradev/uira';

export async function installHooks(cwd?: string): Promise<void> {
  const { execSync } = await import('child_process');
  const targetDir = cwd || process.cwd();
  
  try {
    execSync('uira install', { cwd: targetDir, stdio: 'inherit' });
  } catch {
    console.error('Failed to install git hooks. Make sure uira CLI is available.');
    throw new Error('Hook installation failed');
  }
}

export async function runHook(hookName: string, cwd?: string): Promise<boolean> {
  const { execSync } = await import('child_process');
  const targetDir = cwd || process.cwd();
  
  try {
    execSync(`uira run ${hookName}`, { cwd: targetDir, stdio: 'inherit' });
    return true;
  } catch {
    return false;
  }
}
