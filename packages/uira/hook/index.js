/**
 * Uira Git Hooks Module
 * 
 * Provides programmatic access to git hook functionality.
 * 
 * @example
 * ```js
 * import { install, run } from 'uira/hook';
 * 
 * // Install git hooks
 * await install();
 * 
 * // Run a specific hook
 * await run('pre-commit');
 * ```
 */

import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));

/**
 * Execute the uira CLI with given arguments
 * @param {string[]} args - CLI arguments
 * @param {object} options - Spawn options
 * @returns {Promise<{code: number, stdout: string, stderr: string}>}
 */
function execUira(args, options = {}) {
  return new Promise((resolve, reject) => {
    const binPath = join(__dirname, '..', 'bin', 'uira.js');
    
    // Use node to execute the JS wrapper
    const child = spawn(process.execPath, [binPath, ...args], {
      stdio: options.stdio || 'pipe',
      cwd: options.cwd || process.cwd(),
      env: { ...process.env, ...options.env },
    });
    
    let stdout = '';
    let stderr = '';
    
    if (child.stdout) {
      child.stdout.on('data', (data) => { stdout += data.toString(); });
    }
    if (child.stderr) {
      child.stderr.on('data', (data) => { stderr += data.toString(); });
    }
    
    child.on('error', reject);
    child.on('exit', (code) => {
      resolve({ code: code ?? 0, stdout, stderr });
    });
  });
}

/**
 * Install git hooks to .git/hooks/
 * 
 * Creates hook scripts that delegate to uira for execution.
 * Requires uira.yml configuration file in the project root.
 * 
 * @param {object} options
 * @param {string} [options.cwd] - Working directory (default: process.cwd())
 * @returns {Promise<void>}
 * @throws {Error} If installation fails
 */
export async function install(options = {}) {
  const result = await execUira(['install'], {
    cwd: options.cwd,
    stdio: 'inherit',
  });
  
  if (result.code !== 0) {
    throw new Error(`Hook installation failed with code ${result.code}`);
  }
}

/**
 * Run a specific git hook
 * 
 * Executes the commands defined for the specified hook in uira.yml.
 * 
 * @param {string} hookName - Hook name (e.g., 'pre-commit', 'post-commit')
 * @param {object} options
 * @param {string} [options.cwd] - Working directory (default: process.cwd())
 * @returns {Promise<void>}
 * @throws {Error} If hook execution fails
 */
export async function run(hookName, options = {}) {
  const result = await execUira(['run', hookName], {
    cwd: options.cwd,
    stdio: 'inherit',
  });
  
  if (result.code !== 0) {
    throw new Error(`Hook '${hookName}' failed with code ${result.code}`);
  }
}

/**
 * Initialize uira configuration
 * 
 * Creates a default uira.yml configuration file if it doesn't exist.
 * 
 * @param {object} options
 * @param {string} [options.cwd] - Working directory (default: process.cwd())
 * @param {string} [options.config] - Config file path (default: 'uira.yml')
 * @returns {Promise<void>}
 */
export async function init(options = {}) {
  const args = ['init'];
  if (options.config) {
    args.push('--config', options.config);
  }
  
  const result = await execUira(args, {
    cwd: options.cwd,
    stdio: 'inherit',
  });
  
  if (result.code !== 0) {
    throw new Error(`Initialization failed with code ${result.code}`);
  }
}

/**
 * Check all configured goals
 * 
 * Runs score-based verification goals defined in uira.yml.
 * 
 * @param {object} options
 * @param {string} [options.cwd] - Working directory (default: process.cwd())
 * @param {string} [options.name] - Specific goal name to check (optional)
 * @returns {Promise<{passed: boolean, stdout: string, stderr: string}>}
 */
export async function checkGoals(options = {}) {
  const args = ['goals', 'check'];
  if (options.name) {
    args.push(options.name);
  }
  
  const result = await execUira(args, {
    cwd: options.cwd,
  });
  
  return {
    passed: result.code === 0,
    stdout: result.stdout,
    stderr: result.stderr,
  };
}

export default {
  install,
  run,
  init,
  checkGoals,
};
