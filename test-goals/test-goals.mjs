import { checkGoalsFromConfig, listGoalsFromConfig } from '../crates/astrape-napi/index.js';

async function main() {
  const cwd = process.cwd();
  
  console.log('=== Testing NAPI Goal Bindings ===\n');
  
  // Test list goals
  console.log('1. Listing goals from config:');
  const goals = listGoalsFromConfig(cwd);
  console.log(JSON.stringify(goals, null, 2));
  
  // Test check goals
  console.log('\n2. Checking goals:');
  const result = await checkGoalsFromConfig(cwd);
  console.log(JSON.stringify(result, null, 2));
  
  console.log('\n=== Tests Complete ===');
}

main().catch(console.error);
