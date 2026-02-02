import type { CommentInfo, CommentCheckResult } from './types';

export type { CommentInfo, CommentCheckResult };

interface NativeCommentChecker {
  new (): NativeCommentChecker;
  shouldCheckTool(toolName: string): boolean;
  checkWrite(filePath: string, content: string): string | null;
  checkEdit(filePath: string, oldString: string, newString: string): string | null;
  checkToolResult(toolName: string, toolInput: string): string | null;
}

let NativeCommentCheckerClass: NativeCommentChecker | null = null;

try {
  const native = require('../uira.node');
  if (native.CommentChecker) {
    NativeCommentCheckerClass = native.CommentChecker;
  }
} catch {
  NativeCommentCheckerClass = null;
}

const CHECKABLE_TOOLS = ['Write', 'Edit', 'MultiEdit', 'NotebookEdit'];

export class CommentChecker {
  private native: NativeCommentChecker | null = null;

  constructor() {
    if (NativeCommentCheckerClass) {
      this.native = new (NativeCommentCheckerClass as unknown as new () => NativeCommentChecker)();
    }
  }

  shouldCheckTool(toolName: string): boolean {
    if (this.native) {
      return this.native.shouldCheckTool(toolName);
    }
    return CHECKABLE_TOOLS.includes(toolName);
  }

  checkWrite(filePath: string, content: string): string | null {
    if (!this.native) {
      return null;
    }
    return this.native.checkWrite(filePath, content);
  }

  checkEdit(filePath: string, oldString: string, newString: string): string | null {
    if (!this.native) {
      return null;
    }
    return this.native.checkEdit(filePath, oldString, newString);
  }

  checkToolResult(toolName: string, toolInput: object): string | null {
    if (!this.native) {
      return null;
    }
    return this.native.checkToolResult(toolName, JSON.stringify(toolInput));
  }
}
