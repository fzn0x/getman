// Runs kept in the workspace's .getman folder: the list the Runner shows,
// and their reports as downloads.

import { api } from "./api";
import { message, notify } from "./store.svelte";
import type { ReportFormat, RunAbout, RunListing, RunSummary } from "./types";

export const pastRuns = $state({ list: [] as RunListing[], loaded: false });

export async function refreshRuns(): Promise<void> {
  try {
    pastRuns.list = await api.runs();
    pastRuns.loaded = true;
  } catch {
    // The list is a convenience; keep what is shown.
  }
}

export const REPORT_FORMATS: { format: ReportFormat; label: string }[] = [
  { format: "html", label: "HTML page" },
  { format: "xlsx", label: "Excel workbook" },
  { format: "csv", label: "CSV" },
  { format: "json", label: "JSON" },
  { format: "junit", label: "JUnit XML" },
];

/** Saves a kept run's report through the browser's downloads. */
export async function downloadReport(id: string, format: ReportFormat): Promise<void> {
  try {
    const { blob, name } = await api.runReport(id, format);
    const link = document.createElement("a");
    link.href = URL.createObjectURL(blob);
    link.download = name;
    link.click();
    setTimeout(() => URL.revokeObjectURL(link.href), 1000);
  } catch (error) {
    notify(message(error), "error");
  }
}

export async function forgetRun(id: string): Promise<void> {
  try {
    await api.deleteRun(id);
    await refreshRuns();
  } catch (error) {
    notify(message(error), "error");
  }
}

/** What a run covered, in a few words. */
export function runWhat(about: RunAbout, workspace: string): string {
  const targets = about.targets.filter((t) => t !== "");
  if (targets.length === 0) return workspace || "Every request";
  if (targets.length === 1) return targets[0]!;
  return `${targets.length} picked requests`;
}

/** Passed and total requests of a run. */
export function runTally(summary: RunSummary | null): { passed: number; total: number } {
  if (!summary) return { passed: 0, total: 0 };
  return {
    passed: summary.requests - summary.failed_requests,
    total: summary.requests,
  };
}

/** Whether a run passed: nothing failed and it was not stopped. */
export function runPassed(summary: RunSummary | null): boolean {
  return (
    summary !== null &&
    summary.failed_requests === 0 &&
    summary.tests_failed === 0 &&
    summary.checks_failed === 0 &&
    summary.stopped === null
  );
}
