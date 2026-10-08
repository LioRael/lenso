import { defineApp, definePlugin } from "lenso";

const greeting = definePlugin({
  id: "greeting",
  setup: () => ({
    async greet({ name }: { name: string }) {
      const trimmed = name.trim();
      if (trimmed.length < 2) throw new Error("Name must contain at least 2 characters");
      return { message: `${process.env.GREETING_PREFIX ?? "Hello"}, ${trimmed}!` };
    },
  }),
});

export default defineApp({ plugins: [greeting] });
