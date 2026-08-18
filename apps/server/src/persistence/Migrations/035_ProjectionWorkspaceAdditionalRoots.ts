import * as Effect from "effect/Effect";
import * as SqlClient from "effect/unstable/sql/SqlClient";

export default Effect.gen(function* () {
  const sql = yield* SqlClient.SqlClient;

  const projectColumns = yield* sql<{ readonly name: string }>`
    PRAGMA table_info(projection_projects)
  `;
  if (!projectColumns.some((column) => column.name === "additional_roots_json")) {
    yield* sql`
      ALTER TABLE projection_projects
      ADD COLUMN additional_roots_json TEXT NOT NULL DEFAULT '[]'
    `;
  }

  const threadColumns = yield* sql<{ readonly name: string }>`
    PRAGMA table_info(projection_threads)
  `;
  if (!threadColumns.some((column) => column.name === "additional_roots_json")) {
    yield* sql`
      ALTER TABLE projection_threads
      ADD COLUMN additional_roots_json TEXT NOT NULL DEFAULT '[]'
    `;
  }
});
