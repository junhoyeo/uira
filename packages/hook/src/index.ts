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

const VALID_HOOK_NAMES = ['pre-commit', 'commit-msg', 'pre-push', 'post-commit', 'post-merge'];

function validateHookName(hookName: string): void {
  if (!VALID_HOOK_NAMES.includes(hookName)) {
    throw new Error(
      `Invalid hook name: ${hookName}. Valid names are: ${VALID_HOOK_NAMES.join(', ')}`
    );
  }
}

export async function installHooks(cwd?: string): Promise<void> {
  const { spawnSync } = await import('child_process');
  const targetDir = cwd || process.cwd();

  const result = spawnSync('uira', ['install'], {
    cwd: targetDir,
    stdio: 'inherit',
  });

  if (result.error) {
    console.error('Failed to install git hooks. Make sure uira CLI is available.');
    throw new Error('Hook installation failed');
  }

  if (result.status !== 0) {
    throw new Error(`Hook installation failed with exit code ${result.status}`);
  }
}

export async function runHook(hookName: string, cwd?: string): Promise<boolean> {
  validateHookName(hookName);

  const { spawnSync } = await import('child_process');
  const targetDir = cwd || process.cwd();

  const result = spawnSync('uira', ['run', hookName], {
    cwd: targetDir,
    stdio: 'inherit',
  });

  if (result.error) {
    return false;
  }

  return result.status === 0;
}
