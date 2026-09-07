// Installed by the-office. Recommended set only — type-aware rules are a
// separate, later task because they need a resolved tsconfig and cost real
// wall-clock on every run.
import js from '@eslint/js';

export default [
  js.configs.recommended,
  {
    ignores: ['dist/**', 'build/**', 'coverage/**', 'node_modules/**'],
  },
  {
    rules: {
      'no-console': ['warn', { allow: ['warn', 'error'] }],
      'no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
      eqeqeq: ['error', 'always', { null: 'ignore' }],
      'prefer-const': 'error',
    },
  },
];
