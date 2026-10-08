import { defineApp } from "lenso";
import { greeting } from "./src/server";
export default defineApp({ plugins: [greeting] });
