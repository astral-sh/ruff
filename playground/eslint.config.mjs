import react from "eslint-plugin-react";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import importPlugin from "eslint-plugin-import";

export default tseslint.config(
  tseslint.configs.eslintRecommended,
  tseslint.configs.recommended,
  reactHooks.configs["recommended-latest"],
  importPlugin.flatConfigs.recommended,
  importPlugin.flatConfigs.typescript,
  {
    ...react.configs.flat.recommended,
    ...react.configs.flat["jsx-runtime"],
    files: ["**/*.{js,mjs,cjs,jsx,mjsx,ts,tsx,mtsx}"],
    languageOptions: {
      ...react.configs.flat.recommended.languageOptions,
      ecmaVersion: "latest",
      sourceType: "module",
    },
    rules: {
      eqeqeq: [
        "error",
        "always",
        {
          null: "never",
        },
      ],
      "@typescript-eslint/no-explicit-any": "off",
      // Handled by typescript. It doesn't support shared?
      "import/no-unresolved": "off",
      "no-console": "error",
    },
  },
  {
    ignores: ["src/pkg/**"],
  },
);
