import { createR2StoragePlugin } from "@lenso/storage/r2";

/** Call inside the existing Worker app factory with explicitly configured bindings. */
export function createStorageInstances(bindings: {
  PUBLIC_ASSETS: R2Bucket;
  PRIVATE_FILES: R2Bucket;
}) {
  const publicAssets = createR2StoragePlugin({
    id: "publicAssets",
    binding: bindings.PUBLIC_ASSETS,
  });
  const privateFiles = createR2StoragePlugin({
    id: "privateFiles",
    binding: bindings.PRIVATE_FILES,
  });
  return { publicAssets, privateFiles, plugins: [publicAssets, privateFiles] };
}
