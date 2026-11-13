import { defineConfig } from "vite";
export function proxyFor(targets: Record<"typescript" | "rust", string>) {
  return Object.fromEntries(Object.entries(targets).map(([name, target]) => [`/backends/${name}`, {
    target, changeOrigin: false,
    rewrite: (path: string) => path.replace(`/backends/${name}`, ""),
  }]));
}
const proxy = proxyFor({ typescript: process.env.TEAM_DESK_TS_URL ?? "http://127.0.0.1:3000", rust: process.env.TEAM_DESK_RUST_URL ?? "http://127.0.0.1:3001" });
export default defineConfig({ server: { proxy }, preview: { proxy } });
