import { and, eq } from "drizzle-orm";
import type { NodePgDatabase } from "drizzle-orm/node-postgres";
import type { SubjectRef } from "@lenso/auth";
import * as schema from "./schema";

export interface OwnershipStore {
  claim(reportId: string, owner: SubjectRef): Promise<SubjectRef | null>;
  report(reportId: string): Promise<SubjectRef | null>;
  job(jobId: string): Promise<SubjectRef | null>;
  record(jobId: string, reportId: string): Promise<void>;
}

export function createOwnershipStore(
  db: NodePgDatabase<typeof schema>,
  queueName: string,
): OwnershipStore {
  const report = async (reportId: string) => {
    const [owner] = await db
      .select({
        realmId: schema.reportOwners.realmId,
        subjectId: schema.reportOwners.subjectId,
      })
      .from(schema.reportOwners)
      .where(eq(schema.reportOwners.reportId, reportId));
    return owner ?? null;
  };
  return {
    report,
    async claim(reportId, owner) {
      // Never adopt legacy reports whose owner was not recorded by this boundary.
      const [legacy] = await db
        .select({ id: schema.reports.reportId })
        .from(schema.reports)
        .where(eq(schema.reports.reportId, reportId));
      if (legacy && !(await report(reportId))) return null;
      await db
        .insert(schema.reportOwners)
        .values({ reportId, ...owner })
        .onConflictDoNothing();
      return report(reportId);
    },
    async job(jobId) {
      const [owner] = await db
        .select({
          realmId: schema.reportOwners.realmId,
          subjectId: schema.reportOwners.subjectId,
        })
        .from(schema.jobReports)
        .innerJoin(
          schema.reportOwners,
          eq(schema.jobReports.reportId, schema.reportOwners.reportId),
        )
        .where(and(eq(schema.jobReports.queueName, queueName), eq(schema.jobReports.jobId, jobId)));
      return owner ?? null;
    },
    async record(jobId, reportId) {
      await db
        .insert(schema.jobReports)
        .values({ queueName, jobId, reportId })
        .onConflictDoNothing();
      const [existing] = await db
        .select()
        .from(schema.jobReports)
        .where(and(eq(schema.jobReports.queueName, queueName), eq(schema.jobReports.jobId, jobId)));
      if (existing?.reportId !== reportId) throw new Error("Job ownership conflict.");
    },
  };
}
