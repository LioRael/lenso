import { defineConfig } from "oxlint";
import core from "ultracite/oxlint/core";
import react from "ultracite/oxlint/react";
import tanstack from "ultracite/oxlint/tanstack";

export default defineConfig({
  extends: [core, react, tanstack],
  overrides: [
    {
      files: [
        "src/**/*.{ts,tsx}",
        "packages/story-console/src/**/*.{ts,tsx}",
        "packages/identity-console/src/**/*.{ts,tsx}",
        "vite.config.ts",
      ],
      rules: {
        complexity: "off",
        "func-style": "off",
        "import/consistent-type-specifier-style": "off",
        "import/no-cycle": "off",
        "no-nested-ternary": "off",
        "no-promise-executor-return": "off",
        "no-use-before-define": "off",
        "promise/avoid-new": "off",
        "require-await": "off",
        "require-unicode-regexp": "off",
        "sort-keys": "off",
        "typescript/array-type": "off",
        "typescript/consistent-type-definitions": "off",
        "typescript/no-floating-promises": "off",
        "typescript/no-non-null-assertion": "off",
        "unicorn/no-array-for-each": "off",
        "unicorn/no-array-reduce": "off",
        "unicorn/no-array-sort": "off",
        "unicorn/no-nested-ternary": "off",
        "unicorn/no-useless-undefined": "off",
        "unicorn/prefer-query-selector": "off",
        "unicorn/prefer-spread": "off",
        "unicorn/prefer-ternary": "off",
      },
    },
  ],
});
