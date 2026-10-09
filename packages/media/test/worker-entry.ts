import { createMedia, MediaError, type MediaOptions } from "@lenso/media";
import { createMediaTask, createTasksMediaAdapter } from "@lenso/media/tasks";
import { createD1MediaStore } from "@lenso/media/d1";
import { createMediaFileJournal, createFilesMediaStorage } from "@lenso/media/files";

/** Workers borrows storage/tasks and delegates to an independently deployed Bun executor. */
export function control<Access>(options: Omit<MediaOptions<Access>, "processor">) {
  return createMedia(options);
}
export {
  MediaError,
  createMediaTask,
  createTasksMediaAdapter,
  createD1MediaStore,
  createMediaFileJournal,
  createFilesMediaStorage,
};
