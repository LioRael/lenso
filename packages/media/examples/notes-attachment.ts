import type { FileReference, Media } from "@lenso/media";

/** The existing Notes service must authorize the note before passing its attachment identity. */
export async function requestNoteThumbnail<Access>(
  media: Media<Access>,
  access: Access,
  attachment: FileReference,
) {
  return media.request(access, { source: attachment, preset: "notes-attachment" });
}

export async function noteThumbnail<Access>(
  media: Media<Access>,
  access: Access,
  derivationId: string,
) {
  const status = await media.status(access, derivationId);
  if (status.state === "ready")
    return { state: "ready" as const, file: await media.result(access, derivationId) };
  return { state: status.state, stage: status.stage, error: status.error };
}
