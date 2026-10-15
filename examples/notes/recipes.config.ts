import { defineApp } from "@lenso/core";
import { createLocalStoragePlugin } from "@lenso/storage/local";
import { createRecipeApplication } from "./src/recipes/application";

// Importing config opens nothing. Paths and principals are owned by this standalone host.
export const recipe = createRecipeApplication({
  filename: process.env.SQLITE_PATH ?? "./recipe.sqlite",
  principals: () => process.env.NOTES_LOGIN_KEYS,
  storage: createLocalStoragePlugin({
    id: "privateFiles",
    root: process.env.STORAGE_ROOT ?? "./recipe-objects",
  }),
});
export default defineApp({ plugins: recipe.plugins });
