// SPDX-License-Identifier: GPL-3.0-or-later
import js from '@eslint/js';
import prettier from 'eslint-config-prettier';
import jsxA11y from 'eslint-plugin-jsx-a11y';
import reactHooks from 'eslint-plugin-react-hooks';
import reactRefresh from 'eslint-plugin-react-refresh';
import globals from 'globals';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  {
    ignores: [
      '**/target/**',
      '**/node_modules/**',
      '**/dist/**',
      '**/coverage/**',
      '**/gen/**',
      'apps/desktop/src/lib/ipc/bindings.ts',
      'docs/.vitepress/cache/**',
      'docs/.vitepress/dist/**',
      'playwright-report/**',
    ],
  },
  js.configs.recommended,
  {
    files: ['**/*.{js,mjs,cjs}'],
    languageOptions: { globals: { ...globals.node } },
  },
  {
    files: ['apps/desktop/isolation/**/*.js'],
    languageOptions: { globals: { ...globals.browser } },
  },
  {
    files: ['apps/**/*.{ts,tsx}'],
    extends: [...tseslint.configs.strictTypeChecked, ...tseslint.configs.stylisticTypeChecked],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
      globals: { ...globals.browser },
    },
    plugins: {
      'react-hooks': reactHooks,
      'react-refresh': reactRefresh,
      'jsx-a11y': jsxA11y,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      ...jsxA11y.flatConfigs.recommended.rules,
      // Fast refresh is a dev-only nicety; feature modules intentionally co-locate helpers.
      'react-refresh/only-export-components': 'off',
      // Dialogs focus their primary field on open, which is the expected behaviour.
      'jsx-a11y/no-autofocus': 'off',
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_' },
      ],
      '@typescript-eslint/restrict-template-expressions': ['error', { allowNumber: true }],
      '@typescript-eslint/consistent-type-imports': 'error',
      '@typescript-eslint/no-confusing-void-expression': ['error', { ignoreArrowShorthand: true }],
      '@typescript-eslint/no-misused-promises': [
        'error',
        { checksVoidReturn: { attributes: false } },
      ],
      'no-console': ['warn', { allow: ['warn', 'error'] }],
      eqeqeq: ['error', 'always'],
    },
  },
  {
    // The browser mock backend throws IPC-shaped error objects like the real core does.
    files: ['apps/desktop/src/lib/ipc/mock*.ts'],
    rules: { '@typescript-eslint/only-throw-error': 'off' },
  },
  {
    files: ['apps/**/*.{test,spec}.{ts,tsx}', 'apps/**/e2e/**/*.ts'],
    rules: {
      '@typescript-eslint/no-non-null-assertion': 'off',
      '@typescript-eslint/no-unsafe-assignment': 'off',
    },
  },
  prettier,
);
