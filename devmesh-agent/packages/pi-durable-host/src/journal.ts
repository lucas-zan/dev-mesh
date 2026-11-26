import { DatabaseSync } from "node:sqlite";

export class Journal {
  private db: DatabaseSync;
  constructor(path: string) {
    this.db = new DatabaseSync(path);
    this.db.exec(
      "PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS records(key TEXT PRIMARY KEY, value TEXT NOT NULL)",
    );
  }
  get<T>(key: string): T | undefined {
    const row = this.db
      .prepare("SELECT value FROM records WHERE key=?")
      .get(key);
    return row ? (JSON.parse(row.value as string) as T) : undefined;
  }
  set(key: string, value: unknown): void {
    this.db
      .prepare(
        "INSERT INTO records VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
      )
      .run(key, JSON.stringify(value));
  }
  close(): void {
    this.db.close();
  }
}
