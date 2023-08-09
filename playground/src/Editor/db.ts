const DB_URL = import.meta.env.PROD
  ? "https://api.astral-1ad.workers.dev"
  : "http://0.0.0.0:8787";

/**
 * Get a value from the database.
 */
export async function get<T>(key: string): Promise<T | null> {
  const response = await fetch(`${DB_URL}/${encodeURIComponent(key)}`);
  if (!response.ok) {
    return null;
  }
  return await response.json();
}

/**
 * Set a value in the database.
 */
export async function set<T>(key: string, value: T): Promise<void> {
  const response = await fetch(`${DB_URL}/${encodeURIComponent(key)}`, {
    method: "POST",
    body: JSON.stringify(value),
  });
  if (!response.ok) {
    throw new Error("Failed to save.");
  }
}
