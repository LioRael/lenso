# lenso-module

`lenso-module` owns portable authoring primitives shared by Lenso Module
frontends. It does not discover providers or mutate an App graph: a typed
`Port<C>` connects only to the `ModuleDependencies` selected by the immutable
Resolved App Plan.
