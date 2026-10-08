import type { StandardSchemaV1 } from "@standard-schema/spec";
import { bindConfig, definePluginConfig, resolveConfig, valuesSource } from "../src/config";
import type { ConfigBinding } from "../src/config-types";
import type { Plugin, PluginContext } from "../src/plugin";

type Equal<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2 ? true : false;
type Assert<T extends true> = T;
type IsAny<T> = 0 extends 1 & T ? true : false;
function assertType<T extends true>(_value: T): void {}
type Input = { port?: string };
type Output = { port: number; enabled: boolean };

const schema: StandardSchemaV1<Input, Output> = {
  "~standard": {
    version: 1,
    vendor: "test",
    validate: () => ({ value: { port: 80, enabled: true } }),
  },
};
const contract = definePluginConfig({ schema });
const plugin = bindConfig(
  contract,
  { port: "80" },
  {
    id: "typed",
    setup(_context, config) {
      assertType<Equal<typeof config, Output>>(true);
      assertType<Equal<IsAny<typeof config>, false>>(true);
      const output: Output = config;
      // @ts-expect-error schema output is not input
      const input: Input = config;
      void input;
      return output.port;
    },
  },
);
export type ServiceIsNumber = Assert<Equal<typeof plugin, Plugin<number>>>;
const sourcePlugin = bindConfig(contract, [valuesSource({ port: "80" })], {
  id: "sources",
  setup: (_context, config) => config.enabled,
});
export type SourceServiceIsBoolean = Assert<Equal<typeof sourcePlugin, Plugin<boolean>>>;
// @ts-expect-error schema input expects a string
bindConfig(contract, { port: 80 }, { id: "invalid", setup: () => undefined });
// @ts-expect-error required config callback sees transformed numeric output
bindConfig(contract, {}, { id: "invalid-output", setup: (_context, config: Input) => config });
const binding: ConfigBinding<typeof schema> = { contract, sources: [] };
const snapshot = resolveConfig("typed", binding);
type SnapshotValue = Awaited<typeof snapshot>["value"];
export type SnapshotIsOutput = Assert<Equal<SnapshotValue, Output>>;
export type SnapshotNotAny = Assert<Equal<IsAny<SnapshotValue>, false>>;
function contextTypes(context: PluginContext): void {
  const config = context.config(binding);
  assertType<Equal<typeof config, Output>>(true);
  assertType<Equal<IsAny<typeof config>, false>>(true);
}
void contextTypes;
