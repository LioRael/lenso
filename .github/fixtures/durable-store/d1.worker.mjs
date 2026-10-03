// Test-owned finite atomic operation. No interactive transaction API is shared.
const schema = [
  "CREATE TABLE IF NOT EXISTS receipts (id TEXT PRIMARY KEY, amount INTEGER NOT NULL CHECK(amount > 0))",
  "CREATE TABLE IF NOT EXISTS effects (id TEXT PRIMARY KEY, amount INTEGER NOT NULL CHECK(amount > 0))",
];

export default {
  async fetch(request, env) {
    const command = await request.json();
    const db = env.DB.withSession("first-primary");
    if (command.action === "migrate") {
      await db.batch(schema.map((sql) => db.prepare(sql)));
      return Response.json({ ready: true });
    }
    if (command.action === "apply") {
      try {
        await db.batch([
          db.prepare("INSERT INTO receipts(id, amount) VALUES (?, ?) ON CONFLICT(id) DO NOTHING")
            .bind(command.id, command.amount),
          db.prepare("INSERT INTO effects(id, amount) SELECT id, CASE WHEN ? = 1 THEN -1 ELSE amount END FROM receipts WHERE id = ? ON CONFLICT(id) DO NOTHING")
            .bind(command.rollback ? 1 : 0, command.id),
        ]);
      } catch (error) {
        // Only the deliberately provoked, documented batch constraint failure
        // proves rollback. Timeouts and arbitrary D1 errors remain unknown.
        if (command.rollback && String(error).includes("CHECK constraint failed")) {
          return Response.json({ kind: "rolled_back", error: "constraint" });
        }
        return Response.json({ kind: "unknown", error: "backend" });
      }
      try {
        const receipt = await db.prepare("SELECT id, amount FROM receipts WHERE id = ?")
          .bind(command.id).first();
        return Response.json({ kind: "committed", value: receipt });
      } catch {
        return Response.json({ kind: "unknown", error: "backend" });
      }
    }
    if (command.action === "read") {
      const [receipts, effects] = await db.batch([
        db.prepare("SELECT id, amount FROM receipts WHERE id = ?").bind(command.id),
        db.prepare("SELECT id, amount FROM effects WHERE id = ?").bind(command.id),
      ]);
      return Response.json({ receipt: receipts.results[0] ?? null, effect: effects.results[0] ?? null });
    }
    throw new Error(`unsupported fixture action: ${command.action}`);
  },
};
