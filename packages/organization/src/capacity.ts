import { OrganizationError, type OrganizationState } from "./contracts";

export const maximumSnapshotBytes = 512_000;

export function assertSnapshotCapacity(state: OrganizationState): void {
  const bytes = new TextEncoder().encode(JSON.stringify(state)).byteLength;
  // Reserve the widest safe integer timestamp replacing `null` on EVERY pending
  // invite, so removal/leave can terminalize them all without growing past limit.
  // Also reserve organization.updatedAt growth if a later clock has more digits.
  const pending = state.invitations.filter((invite) => invite.status === "pending").length;
  const headroom =
    pending * 12 +
    16 -
    String(state.organization.updatedAt).length +
    16 -
    String(state.organization.version).length;
  if (bytes + headroom > maximumSnapshotBytes) throw new OrganizationError("CAPACITY");
}
