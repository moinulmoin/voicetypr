const tsParser = require("@typescript-eslint/parser");
const reactHooks = require("eslint-plugin-react-hooks");

module.exports = [
  {
    ignores: ["dist/", "public/", "node_modules/", "src-tauri/**"],
  },
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 2020,
        sourceType: "module",
        ecmaFeatures: { jsx: true },
      },
    },
    plugins: { "react-hooks": reactHooks },
    rules: {
      ...reactHooks.configs.recommended.rules,
      // Off in the repo's prior config; oxlint/react-doctor cover churn instead.
      "react-hooks/exhaustive-deps": "off",
      "react-hooks/preserve-manual-memoization": "off",
      "react-hooks/purity": "off",
    },
  },
];
