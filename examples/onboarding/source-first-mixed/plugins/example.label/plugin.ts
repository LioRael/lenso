import { configuration, definePlugin } from "@lenso/bun-plugin/authoring";
import { Metadata, type InvocationContext, type NormalizeRequest } from "../../contracts/metadata/generated.ts";

export default definePlugin({
  config: configuration({ type: "object", properties: { label: { type: "string" } }, required: ["label"], additionalProperties: false },
    value => value as { label: string }),
  dependencies: { upstream: Metadata.optional("upstream") },
  provides: [Metadata],
  maxConcurrentRequests: 1,
  create({ config, dependencies }) {
    let calls = 0;
    return {
      async normalize(context: InvocationContext, request: NormalizeRequest) {
        calls++;
        let name = request.name;
        if (dependencies.upstream) {
          const result = await dependencies.upstream.normalize(request, context);
          if (!result.ok) return result;
          name = result.value.name;
        }
        return { ok: true as const, value: { name: `${config.label}:${calls}:${name}`, extension: "none", size_bytes: request.size_bytes } };
      },
    };
  },
});
