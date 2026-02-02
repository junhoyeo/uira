#!/usr/bin/env node

const args = process.argv.slice(2);
const command = args[0];

async function main() {
  const { installHooks, runHook } = await import('@uiradev/uira/hooks');

  switch (command) {
    case 'install':
      installHooks({ preCommit: true, prePush: true });
      console.log('✓ Git hooks installed');
      break;

    case 'uninstall':
      installHooks({ preCommit: false, prePush: false, commitMsg: false });
      console.log('✓ Git hooks uninstalled');
      break;

    case 'run':
      const hookName = args[1];
      if (!hookName) {
        console.error('Usage: uira-hook run <hook-name>');
        process.exit(1);
      }
      const success = runHook(hookName);
      process.exit(success ? 0 : 1);

    case 'help':
    case '--help':
    case '-h':
      printHelp();
      break;

    default:
      if (command) {
        console.error(`Unknown command: ${command}`);
        console.error('');
      }
      printHelp();
      process.exit(command ? 1 : 0);
  }
}

function printHelp() {
  console.log(`uira-hook - Git hooks made easy with AI assistance

Usage: uira-hook <command> [options]

Commands:
  install     Install git hooks to .git/hooks/
  uninstall   Remove git hooks
  run <hook>  Run a specific hook (e.g., pre-commit)
  help        Show this help message

Examples:
  uira-hook install
  uira-hook run pre-commit

Add to package.json:
  {
    "scripts": {
      "prepare": "uira-hook install"
    }
  }
`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
