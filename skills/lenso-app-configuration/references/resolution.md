# Resolution and Generations

Resolution is a pure operation over one immutable Host Catalog and one complete
Plugin Root snapshot. Before Plan materialization, Host policy selects one
compatible executable implementation from each Plugin Release. Resolution
merges package defaults, Host configuration, and the Instance patch; validates
the final value; closes root Slots and requirements; and produces one immutable
Resolved App Plan.

The App owner does not name an implementation, execution class, provider key,
lane, endpoint, requirement, or binding in `plugins/`. An unsupported or
ambiguous implementation set is a Host-policy error. The resolver never uses
discovery order. A selected implementation failure never causes runtime fallback.

The Kernel receives only that Plan. It does not discover packages, read the
Plugin Root, select implementations or providers, or mutate its graph.

A live Host stages a changed Plan as a candidate Generation. The Controller
prepares resources and requires readiness before switching new routes. Existing
Turns keep their exact Generation lease until terminal completion. Invalid input
or readiness failure leaves current routing unchanged.

Generation, Controller, Supervisor, Receipt, Store, and resolution authority
are internal operational concepts. App owners do not author them.
