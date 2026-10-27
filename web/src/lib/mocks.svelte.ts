// Mock servers started from the UI, one per folder at most.

import { api } from "./api";
import { message, notify } from "./store.svelte";
import type { MockServer } from "./types";

export const mocks = $state({ list: [] as MockServer[] });

export function mockFor(folder: string): MockServer | undefined {
  return mocks.list.find((mock) => mock.folder === folder);
}

export async function refreshMocks(): Promise<void> {
  try {
    mocks.list = await api.mocks();
  } catch {
    // Keep what is shown; the next refresh tries again.
  }
}

export async function startMock(folder: string): Promise<void> {
  try {
    const started = await api.startMock(folder);
    mocks.list = [...mocks.list.filter((m) => m.folder !== folder), started];
    if (started.mocked.length === 0) {
      notify(
        "The mock server is running, but nothing here has examples yet. Save a response as an example and it is served right away.",
        "info",
      );
    }
  } catch (error) {
    notify(message(error), "error");
  }
}

export async function stopMock(folder: string): Promise<void> {
  try {
    await api.stopMock(folder);
    mocks.list = mocks.list.filter((m) => m.folder !== folder);
  } catch (error) {
    notify(message(error), "error");
  }
}
