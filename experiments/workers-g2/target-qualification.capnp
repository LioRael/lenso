using Workerd = import "/workerd/workerd.capnp";
const config :Workerd.Config = (
  services = [
    (name = "callback", worker = .callback),
    (name = "target", worker = .target)
  ]
);
const callback :Workerd.Worker = (
  modules = [(name = "main.mjs", esModule = embed "target-callback-service.mjs")],
  compatibilityDate = "2026-07-08",
  compatibilityFlags = ["global_fetch_strictly_public", "enable_request_signal"]
);
const target :Workerd.Worker = (
  modules = [
    (name = "qualification.mjs", esModule = embed ".w02/target-qualification-workerd.mjs"),
    (name = "pkg/lenso_workers_g2_host_bg.wasm", wasm = embed "pkg/lenso_workers_g2_host_bg.wasm")
  ],
  compatibilityDate = "2026-07-08",
  compatibilityFlags = ["global_fetch_strictly_public", "enable_request_signal", "nodejs_compat"],
  bindings = [(name = "POSTGRES_CALLBACK", service = "callback")]
);
