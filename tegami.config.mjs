/**
 * Lenso crates are independently versioned. Cargo dependency propagation keeps
 * published dependency ranges valid without enabling grouped Version PRs.
 */
export default {
  ignore: [
    "@lenso/framework-release-workspace",
    "arch-check",
    "generate-contracts",
    "lenso-operator",
    "otel-smoke",
    "provider-fixture",
  ],
  packages: {
    "lenso": {},
    "lenso-api": {},
    "lenso-bootstrap": {},
    "lenso-contracts": {},
    "lenso-migrate": {},
    "lenso-platform-admin": {},
    "lenso-platform-admin-data": {},
    "lenso-platform-core": {},
    "lenso-platform-http": {},
    "lenso-platform-module": {},
    "lenso-platform-provider": {},
    "lenso-platform-runtime": {},
    "lenso-platform-testing": {},
    "lenso-service": {},
    "lenso-worker": {},
  },
};
