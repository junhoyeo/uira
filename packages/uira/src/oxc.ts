import type { LintDiagnostic, LintRule, LinterConfig } from './types';

export type { LintDiagnostic, LintRule, LinterConfig };

interface NativeLinter {
  new (rules?: string[]): NativeLinter;
  lintFiles(files: string[]): LintDiagnostic[];
  lintSource(filename: string, source: string): LintDiagnostic[];
}

interface NativeLinterStatic {
  new (rules?: string[]): NativeLinter;
  recommended(): NativeLinter;
  strict(): NativeLinter;
  allRules(): string[];
  recommendedRules(): string[];
}

let NativeLinterClass: NativeLinterStatic | null = null;

try {
  const native = require('../uira.node');
  if (native.Linter) {
    NativeLinterClass = native.Linter;
  }
} catch {
  NativeLinterClass = null;
}

const RECOMMENDED_RULES: LintRule[] = [
  'no-debugger',
  'no-eval',
  'no-var',
  'no-duplicate-keys',
];

const ALL_RULES: LintRule[] = [
  'no-console',
  'no-debugger',
  'no-alert',
  'no-eval',
  'no-var',
  'prefer-const',
  'no-unused-vars',
  'no-empty-function',
  'no-duplicate-keys',
  'no-param-reassign',
];

export class Linter {
  private native: NativeLinter | null = null;
  private rules: Set<LintRule>;

  constructor(config: LinterConfig = {}) {
    this.rules = new Set(config.rules ?? RECOMMENDED_RULES);
    
    if (NativeLinterClass) {
      const ruleStrings = config.rules ?? RECOMMENDED_RULES;
      this.native = new NativeLinterClass(ruleStrings);
    }
  }

  static recommended(): Linter {
    const linter = new Linter({ rules: RECOMMENDED_RULES });
    if (NativeLinterClass) {
      linter.native = NativeLinterClass.recommended();
    }
    return linter;
  }

  static strict(): Linter {
    const linter = new Linter({ rules: ALL_RULES });
    if (NativeLinterClass) {
      linter.native = NativeLinterClass.strict();
    }
    return linter;
  }

  static allRules(): LintRule[] {
    if (NativeLinterClass) {
      return NativeLinterClass.allRules() as LintRule[];
    }
    return [...ALL_RULES];
  }

  static recommendedRules(): LintRule[] {
    if (NativeLinterClass) {
      return NativeLinterClass.recommendedRules() as LintRule[];
    }
    return [...RECOMMENDED_RULES];
  }

  hasRule(rule: LintRule): boolean {
    return this.rules.has(rule);
  }

  lintFiles(files: string[]): LintDiagnostic[] {
    if (!this.native) {
      console.warn('@uiradev/uira: Native linter not available. Returning empty diagnostics.');
      return [];
    }
    return this.native.lintFiles(files);
  }

  lintSource(filename: string, source: string): LintDiagnostic[] {
    if (!this.native) {
      console.warn('@uiradev/uira: Native linter not available. Returning empty diagnostics.');
      return [];
    }
    return this.native.lintSource(filename, source);
  }
}
