import { z } from "zod";
import { reportInput } from "./report-service";

export const submitInput = z
  .object({
    ...reportInput.shape,
    runAt: z.iso.datetime({ offset: true }).optional(),
    deduplicationKey: z.string().min(1).max(200).optional(),
  })
  .strict();

export const jobInput = z.object({ jobId: z.uuid() }).strict();
export const reportQueryInput = z.object({ reportId: reportInput.shape.reportId }).strict();
