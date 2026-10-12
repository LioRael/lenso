// Kept out of package exports: provenance is checked by instance-bound adapters.
export const accessOwners = new WeakMap<object, { readonly source: object }>();
