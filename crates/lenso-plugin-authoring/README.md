# lenso-plugin-authoring

`lenso-plugin-authoring` owns runtime-neutral authoring primitives shared by
Lenso Plugin frontends. It does not discover providers or mutate an App graph:
a typed `Port<C>` connects only through the adapter-owned dependency view
implemented by its generated `CapabilityClient`.

Generated clients also expose an exact `CapabilityReference<Client>` and can
connect through a source-named requirement. Provider selection remains owned by
the resolved App Plan and Adapter dependency view.
