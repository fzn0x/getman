// The saved examples of requests, by path. Filled when a request opens
// and after every change, so the Examples tab and its count stay current.

import { api } from "./api";
import type { Example } from "./types";

export const examples = $state({ byPath: {} as Record<string, Example[]> });

export async function loadExamples(path: string): Promise<void> {
  examples.byPath[path] = await api.examples(path);
}
