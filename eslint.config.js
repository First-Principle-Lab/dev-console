import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import svelteESLintParser from 'svelte-eslint-parser';
import globals from 'globals';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  { ignores: ['dist/**', 'src-tauri/target/**', 'src-tauri/gen/**', 'test-results/**', 'playwright-report/**', 'node_modules/**'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...svelte.configs['flat/recommended'],
  {
    files: ['**/*.ts'],
    languageOptions: {
      parser: tseslint.parser,
      parserOptions: { ecmaVersion: 2022, sourceType: 'module' }
    }
  },
  {
    files: ['**/*.svelte'],
    languageOptions: {
      parser: svelteESLintParser,
      parserOptions: {
        parser: tseslint.parser,
        ecmaVersion: 2022,
        sourceType: 'module'
      }
    }
  },
  {
    files: ['**/*.{js,ts,svelte}'],
    languageOptions: {
      globals: { ...globals.browser, ...globals.es2022 }
    },
    rules: {
      'svelte/no-at-html-tags': 'error',
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
      'no-console': ['error', { allow: ['warn', 'error'] }]
    }
  },
  {
    files: ['**/*.config.{js,ts}', 'scripts/**/*.{js,ts,mjs}', 'tests/**/*.{ts,svelte,js}'],
    languageOptions: { globals: { ...globals.node, ...globals.browser } },
    rules: { 'no-console': 'off' }
  }
);
