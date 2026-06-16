#!/usr/bin/env node
import { serveHelloActionModule } from "./module.mjs";

const server = await serveHelloActionModule({ port: 0 });

try {
  const manifest = await fetch(server.manifestUrl).then((response) =>
    response.json()
  );
  if (manifest.name !== "hello-action") {
    throw new Error("manifest did not return hello-action");
  }

  const hello = await fetch(`${server.baseUrl}/hello/Ada`).then((response) =>
    response.json()
  );
  if (hello.message !== "Hello, Ada.") {
    throw new Error("HTTP route did not return the expected greeting");
  }

  const runtime = await fetch(
    `${server.baseUrl}/runtime/functions/hello-action.say-hello.v1/invoke`,
    {
      body: JSON.stringify({
        actor: { id: "release-demo", kind: "service", scopes: [] },
        attempt: 1,
        correlation_id: "corr_release_demo",
        function_name: "hello-action.say-hello.v1",
        function_run_id: "fnrun_release_demo",
        input: { name: "Runtime" },
        request_id: "req_release_demo",
        trace: { span_id: "span_release_demo", trace_id: "trace_release_demo" },
      }),
      headers: { "content-type": "application/json" },
      method: "POST",
    }
  ).then((response) => response.json());
  if (runtime.output?.message !== "Hello, Runtime.") {
    throw new Error("runtime function did not return the expected greeting");
  }

  const admin = await fetch(`${server.baseUrl}/admin/greetings`).then(
    (response) => response.json()
  );
  if (admin.records?.[0]?.recipient !== "release-candidate") {
    throw new Error("schema-admin endpoint did not return greetings");
  }

  console.log("Hello Action remote module smoke passed");
} finally {
  await server.close();
}
