/**
 * Uira Git Hooks Module
 * 
 * Provides programmatic access to git hook functionality.
 */

export interface InstallOptions {
  /** Working directory (default: process.cwd()) */
  cwd?: string;
}

export interface RunOptions {
  /** Working directory (default: process.cwd()) */
  cwd?: string;
}

export interface InitOptions {
  /** Working directory (default: process.cwd()) */
  cwd?: string;
  /** Config file path (default: 'uira.yml') */
  config?: string;
}

export interface CheckGoalsOptions {
  /** Working directory (default: process.cwd()) */
  cwd?: string;
  /** Specific goal name to check (optional) */
  name?: string;
}

export interface CheckGoalsResult {
  passed: boolean;
  stdout: string;
  stderr: string;
}

/**
 * Install git hooks to .git/hooks/
 * 
 * Creates hook scripts that delegate to uira for execution.
 * Requires uira.yml configuration file in the project root.
 */
export function install(options?: InstallOptions): Promise<void>;

/**
 * Run a specific git hook
 * 
 * Executes the commands defined for the specified hook in uira.yml.
 * 
 * @param hookName - Hook name (e.g., 'pre-commit', 'post-commit')
 */
export function run(hookName: string, options?: RunOptions): Promise<void>;

/**
 * Initialize uira configuration
 * 
 * Creates a default uira.yml configuration file if it doesn't exist.
 */
export function init(options?: InitOptions): Promise<void>;

/**
 * Check all configured goals
 * 
 * Runs score-based verification goals defined in uira.yml.
 */
export function checkGoals(options?: CheckGoalsOptions): Promise<CheckGoalsResult>;

declare const hooks: {
  install: typeof install;
  run: typeof run;
  init: typeof init;
  checkGoals: typeof checkGoals;
};

export default hooks;
