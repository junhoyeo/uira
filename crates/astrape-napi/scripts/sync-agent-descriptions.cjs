#!/usr/bin/env node
/**
 * Syncs agent descriptions in markdown files with actual configured models from astrape.yml
 *
 * This script reads the model configuration from astrape.yml and updates the description
 * field in each agent's markdown frontmatter to show the actual configured model.
 */
const fs = require('fs');
const path = require('path');

const AGENTS_DIR = path.resolve(__dirname, '../../../packages/claude-plugin/agents');

// Default models for each agent (matches definitions.rs)
const DEFAULT_MODELS = {
  'architect': 'opus',
  'librarian': 'opencode/big-pickle',
  'explore': 'haiku',
  'designer': 'sonnet',
  'writer': 'haiku',
  'vision': 'sonnet',
  'critic': 'opus',
  'analyst': 'sonnet',
  'executor': 'sonnet',
  'planner': 'opus',
  'qa-tester': 'opus',
  'scientist': 'sonnet',
  'architect-medium': 'sonnet',
  'architect-low': 'haiku',
  'executor-high': 'opus',
  'executor-low': 'haiku',
  'designer-low': 'haiku',
  'designer-high': 'opus',
  'qa-tester-high': 'opus',
  'scientist-low': 'haiku',
  'scientist-high': 'opus',
  'security-reviewer': 'opus',
  'security-reviewer-low': 'haiku',
  'build-fixer': 'sonnet',
  'build-fixer-low': 'haiku',
  'tdd-guide': 'sonnet',
  'tdd-guide-low': 'haiku',
  'code-reviewer': 'opus',
  'code-reviewer-low': 'haiku',
  'researcher': 'sonnet',
  'researcher-low': 'haiku',
};

// Base descriptions without model suffix
const BASE_DESCRIPTIONS = {
  'architect': 'Architecture & Debugging Advisor. Use for complex problems.',
  'librarian': 'Open-source codebase understanding agent for multi-repository analysis, searching remote codebases, and retrieving official documentation.',
  'explore': 'Fast codebase pattern matching.',
  'designer': 'UI/UX specialist.',
  'writer': 'Technical writing specialist.',
  'vision': 'Visual analysis specialist.',
  'critic': 'Plan/work reviewer.',
  'analyst': 'Pre-planning consultant.',
  'executor': 'Focused executor for implementation tasks.',
  'planner': 'Strategic planner for comprehensive implementation plans.',
  'qa-tester': 'CLI testing specialist.',
  'scientist': 'Data/ML specialist.',
  'architect-medium': 'Architecture & Debugging Advisor - Medium complexity. Use for moderate analysis.',
  'architect-low': 'Quick code questions & simple lookups. Use for simple questions that need fast answers.',
  'executor-high': 'Complex task executor for multi-file changes. Use for tasks requiring deep reasoning.',
  'executor-low': 'Simple single-file task executor. Use for trivial tasks.',
  'designer-low': 'Simple styling and minor UI tweaks. Use for trivial frontend work.',
  'designer-high': 'Complex UI architecture and design systems. Use for sophisticated frontend work.',
  'qa-tester-high': 'Comprehensive production-ready QA testing.',
  'scientist-low': 'Quick data inspection and simple statistics. Use for fast, simple queries.',
  'scientist-high': 'Complex research, hypothesis testing, and ML specialist. Use for deep analysis.',
  'security-reviewer': 'Security vulnerability detection specialist. Use for security audits and code review.',
  'security-reviewer-low': 'Quick security scan specialist. Use for fast security checks on small code changes.',
  'build-fixer': 'Build and TypeScript error resolution specialist. Use for fixing build errors.',
  'build-fixer-low': 'Simple build error fixer. Use for trivial type errors and single-line fixes.',
  'tdd-guide': 'Test-Driven Development specialist. Use for TDD workflows and test coverage.',
  'tdd-guide-low': 'Quick test suggestion specialist. Use for simple test case ideas.',
  'code-reviewer': 'Expert code review specialist. Use for comprehensive code quality review.',
  'code-reviewer-low': 'Quick code quality checker. Use for fast review of small changes.',
  'researcher': 'Documentation and external reference finder.',
  'researcher-low': 'Quick documentation lookups. Use for simple documentation queries.',
};

function findAstrapeYml() {
  let dir = process.cwd();
  while (dir !== path.dirname(dir)) {
    const configPath = path.join(dir, 'astrape.yml');
    if (fs.existsSync(configPath)) {
      return configPath;
    }
    dir = path.dirname(dir);
  }
  return null;
}

function parseSimpleYaml(content) {
  // Simple parser for astrape.yml format:
  // agents:
  //   explore:
  //     model: "opencode/gpt-5-nano"
  const result = { agents: {} };
  const lines = content.split('\n');
  let inAgents = false;
  let currentAgent = null;

  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed === 'agents:') {
      inAgents = true;
      continue;
    }
    if (inAgents && !line.startsWith(' ') && !line.startsWith('\t') && trimmed !== '') {
      inAgents = false;
      currentAgent = null;
    }
    if (inAgents) {
      // Check for agent name (2 spaces indent)
      const agentMatch = line.match(/^  (\w[\w-]*):\s*$/);
      if (agentMatch) {
        currentAgent = agentMatch[1];
        result.agents[currentAgent] = {};
        continue;
      }
      // Check for model property (4 spaces indent)
      if (currentAgent) {
        const modelMatch = line.match(/^    model:\s*["']?([^"'\n]+)["']?\s*$/);
        if (modelMatch) {
          result.agents[currentAgent].model = modelMatch[1];
        }
      }
    }
  }
  return result;
}

function loadModelConfig() {
  const configPath = findAstrapeYml();
  const modelConfig = { ...DEFAULT_MODELS };

  if (configPath) {
    try {
      const content = fs.readFileSync(configPath, 'utf8');
      const config = parseSimpleYaml(content);
      if (config?.agents) {
        for (const [agentName, agentConfig] of Object.entries(config.agents)) {
          if (agentConfig?.model) {
            modelConfig[agentName] = agentConfig.model;
          }
        }
      }
      console.log(`[sync-descriptions] Loaded config from ${configPath}`);
    } catch (e) {
      console.log(`[sync-descriptions] Warning: Could not parse ${configPath}: ${e.message}`);
    }
  } else {
    console.log('[sync-descriptions] No astrape.yml found, using defaults');
  }

  return modelConfig;
}

function updateAgentMarkdown(filePath, modelConfig) {
  const content = fs.readFileSync(filePath, 'utf8');
  const agentName = path.basename(filePath, '.md');

  // Get the model for this agent
  const model = modelConfig[agentName] || DEFAULT_MODELS[agentName] || 'sonnet';

  // Get base description
  const baseDesc = BASE_DESCRIPTIONS[agentName];
  if (!baseDesc) {
    // Unknown agent, skip
    return false;
  }

  // Build new description with model
  const newDescription = `${baseDesc} (${model})`;

  // Replace description in frontmatter
  const descRegex = /^(description:\s*)["']?.*["']?$/m;
  const match = content.match(descRegex);
  if (!match) {
    return false;
  }

  const currentDesc = match[0];
  const newDescLine = `description: ${newDescription}`;

  if (currentDesc === newDescLine) {
    return false; // No change needed
  }

  const newContent = content.replace(descRegex, newDescLine);
  fs.writeFileSync(filePath, newContent);
  return true;
}

function main() {
  const modelConfig = loadModelConfig();

  const files = fs.readdirSync(AGENTS_DIR).filter(f => f.endsWith('.md'));
  let updated = 0;

  for (const file of files) {
    const filePath = path.join(AGENTS_DIR, file);
    if (updateAgentMarkdown(filePath, modelConfig)) {
      console.log(`[sync-descriptions] Updated ${file}`);
      updated++;
    }
  }

  console.log(`[sync-descriptions] Done. Updated ${updated} files.`);
}

main();
