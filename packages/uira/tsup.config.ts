import { defineConfig } from 'tsup';

export default defineConfig({
  entry: [
    'src/index.ts',
    'src/comments.ts',
    'src/hooks.ts',
    'src/oxc.ts',
    'src/agent.ts',
    'src/goals.ts',
  ],
  format: ['cjs', 'esm'],
  dts: true,
  clean: true,
  splitting: false,
  external: [/\.node$/],
  noExternal: [],
});
