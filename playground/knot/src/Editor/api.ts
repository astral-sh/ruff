const API_URL = import.meta.env.PROD
  ? "https://api.astral-1ad.workers.dev"
  : "http://0.0.0.0:8787";

export type Playground = {
  files: { [name: string]: string };
  /// the name of the current file
  current: string;
};

/**
 * Fetch a playground by ID.
 */
export async function fetchPlayground(id: string): Promise<Playground | null> {
  const response = await fetch(`${API_URL}/${encodeURIComponent(id)}`);

  if (!response.ok) {
    throw new Error(`Failed to fetch playground ${id}: ${response.status}`);
  }

  return await response.json();
}

/**
 * Save a playground and return its ID.
 */
export async function savePlayground(playground: Playground): Promise<string> {
  const response = await fetch(API_URL, {
    method: "POST",
    body: JSON.stringify(playground),
  });

  if (!response.ok) {
    throw new Error(`Failed to save playground: ${response.status}`);
  }

  return await response.text();
}
