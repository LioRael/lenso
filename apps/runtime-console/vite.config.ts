import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes("node_modules/gsap")) {
            return "gsap";
          }
        },
      },
    },
  },
  plugins: [react(), tailwindcss()],
  server: {
    port: 5174,
  },
});
