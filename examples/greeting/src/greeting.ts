import { definePlugin } from 'lenso/plugin';
export interface GreetingService {
  greet(input: { name: string }): Promise<{ message: string; count: number }>;
}
export const greeting = definePlugin<GreetingService>({
  id: 'greeting',
  setup() {
    let count = 0;
    return {
      async greet({ name }) {
        const trimmed = name.trim();
        if (trimmed.length < 2) throw new Error('Name must contain at least 2 characters');
        return { message: `Hello, ${trimmed}!`, count: ++count };
      },
    };
  },
});
