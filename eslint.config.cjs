const { FlatCompat } = require('@eslint/eslintrc');
const { configs: jsConfigs } = require('@eslint/js');
const tsParser = require('@typescript-eslint/parser');

const compat = new FlatCompat({
  baseDirectory: __dirname,
  recommendedConfig: jsConfigs.recommended,
});

const baseExtends = [
  'eslint:recommended',
  'plugin:@typescript-eslint/recommended',
  'plugin:@typescript-eslint/recommended-requiring-type-checking',
  'plugin:react/recommended',
  'plugin:react-hooks/recommended',
  'plugin:jsx-a11y/recommended',
  'prettier',
];

module.exports = [
  ...baseExtends.flatMap((configName) => compat.extends(configName)),
    {
      ignores: ['dist/', 'public/', 'node_modules/', 'src-tauri/**'],
      languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 2020,
        sourceType: 'module',
        project: ['tsconfig.json', 'tsconfig.node.json'],
        tsconfigRootDir: __dirname,
        ecmaFeatures: {
          jsx: true,
        },
      },
    },
    settings: {
      react: {
        version: 'detect',
      },
    },
    rules: {
      'react/react-in-jsx-scope': 'off',
      'jsx-a11y/no-autofocus': 'off',
      'react/prop-types': 'off',
      'jsx-a11y/label-has-associated-control': 'off',
      'react/no-unescaped-entities': 'off',
      'jsx-a11y/click-events-have-key-events': 'off',
      'jsx-a11y/no-static-element-interactions': 'off',
      'react-hooks/exhaustive-deps': 'off',
      'react-hooks/preserve-manual-memoization': 'off',
      'react-hooks/purity': 'off',
      '@typescript-eslint/explicit-module-boundary-types': 'off',
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unsafe-return': 'off',
      '@typescript-eslint/no-unsafe-call': 'off',
      '@typescript-eslint/no-unsafe-member-access': 'off',
      '@typescript-eslint/no-unsafe-assignment': 'off',
      '@typescript-eslint/no-unused-vars': 'off',
      '@typescript-eslint/no-unsafe-argument': 'off',
      '@typescript-eslint/no-unsafe-function-type': 'off',
      '@typescript-eslint/no-misused-promises': 'off',
      '@typescript-eslint/no-floating-promises': 'off',
      '@typescript-eslint/require-await': 'off',
      '@typescript-eslint/restrict-template-expressions': 'off',
      '@typescript-eslint/await-thenable': 'off',
      '@typescript-eslint/no-unnecessary-type-assertion': 'off',
      '@typescript-eslint/ban-ts-comment': 'off',
      'prefer-const': 'off',
    },
  },
];
