import { createRedisProvider } from "../src/redis";

const provider = createRedisProvider({
  url: process.argv[2]!,
  namespace: process.argv[3]!,
});
const output = (value: unknown) => console.log(JSON.stringify(value));
await provider.start(
  (delivery) => output({ delivery }),
  () => output({ failed: true }),
);
output({ ready: true, pid: process.pid });
for await (const chunk of Bun.stdin.stream()) {
  if (chunk.length) break;
}
await provider.close();
output({ closed: true });
// Bun's stdin stream keeps the child alive after its loop ends.
process.exit(0);
