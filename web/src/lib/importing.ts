// Reading files to import, telling their formats apart for the dialog,
// and saving exported files. The server decides the format for real;
// these labels only tell people what they dropped.

/** A file to import, with its path inside a dropped or chosen folder. */
export interface ImportFile {
  path: string;
  text: string;
}

/** Files larger than this are not read. */
const MAX_FILE_SIZE = 64 * 1024 * 1024;
/** Most files read from one folder. */
const MAX_FILES = 5000;

/** A short description of what a text looks like, or "" when empty. */
export function describeText(text: string, name = ""): string {
  const trimmed = text.replace(/^﻿/, "").trimStart();
  if (!trimmed) return "";
  if (/^curl(\s|$)/.test(trimmed)) return "A cURL command, imported as one request";
  if (/\.bru$/i.test(name) || /^(meta|get|post|put|delete|patch|options|head)\s*\{/.test(trimmed))
    return "A Bruno request (.bru)";
  if (/(^|[{,\s])["']?openapi["']?\s*:\s*["']?3/.test(trimmed)) return "An OpenAPI 3 document";
  if (/(^|[{,\s])["']?swagger["']?\s*:\s*["']?2/.test(trimmed)) return "A Swagger 2.0 document";
  if (/^type:\s*\S*\.insomnia\.rest\//m.test(trimmed)) return "An Insomnia v5 export";
  if (!trimmed.startsWith("{") && !trimmed.startsWith("["))
    return "Paste a cURL command, or an export, spec or HAR file";
  if (/"log"\s*:/.test(trimmed) && /"entries"\s*:/.test(trimmed))
    return "A HAR file: one request per entry";
  if (/"_type"\s*:\s*"export"/.test(trimmed)) return "An Insomnia v4 export";
  if (/"_postman_variable_scope"\s*:\s*"globals"/.test(trimmed)) return "Postman globals";
  if (/"info"\s*:/.test(trimmed) && /"item"\s*:/.test(trimmed)) return "A Postman collection";
  if (/"values"\s*:/.test(trimmed) && !/"item"\s*:/.test(trimmed)) return "A Postman environment";
  if (/"folders"\s*:/.test(trimmed) && /"requests"\s*:/.test(trimmed))
    return "A Hoppscotch collection";
  if (/"variables"\s*:/.test(trimmed)) return "Hoppscotch environments";
  return "JSON in a format Getman does not import";
}

/** A short description of several files, such as a dropped folder. */
export function describeFiles(files: ImportFile[]): string {
  if (files.length === 0) return "";
  const [only] = files;
  if (files.length === 1 && only) return describeText(only.text, only.path);
  const bruno = files.some((f) => /(^|\/)bruno\.json$/i.test(f.path) || /\.bru$/i.test(f.path));
  if (bruno) return `A Bruno collection: ${files.length} files`;
  return `${files.length} files, each imported in its own format`;
}

/** Whether a collection, which can become the workspace itself, is in the text. */
export function isCollection(description: string): boolean {
  return (
    /collection|document|export|HAR/.test(description) &&
    !/^Paste|environment|globals/i.test(description)
  );
}

/** Reads files chosen with an input, keeping folder paths when a folder was chosen. */
export async function readFileList(list: FileList | File[]): Promise<ImportFile[]> {
  const files: ImportFile[] = [];
  for (const file of Array.from(list).slice(0, MAX_FILES)) {
    if (file.size > MAX_FILE_SIZE) continue;
    const path = (file as File & { webkitRelativePath?: string }).webkitRelativePath || file.name;
    files.push({ path, text: await file.text() });
  }
  return files;
}

interface Entry {
  isFile: boolean;
  isDirectory: boolean;
  name: string;
  fullPath: string;
  file?: (ok: (file: File) => void, fail: (error: unknown) => void) => void;
  createReader?: () => {
    readEntries: (ok: (entries: Entry[]) => void, fail: (error: unknown) => void) => void;
  };
}

/** Reads what was dropped: files, and folders with everything in them. */
export async function readDropped(data: DataTransfer): Promise<ImportFile[]> {
  const entries: Entry[] = [];
  for (const item of Array.from(data.items)) {
    const entry = (
      item as DataTransferItem & { webkitGetAsEntry?: () => Entry | null }
    ).webkitGetAsEntry?.();
    if (entry) entries.push(entry);
  }
  if (entries.length === 0) return readFileList(data.files);
  const files: ImportFile[] = [];
  const visit = async (entry: Entry): Promise<void> => {
    if (files.length >= MAX_FILES || entry.name.startsWith(".") || entry.name === "node_modules")
      return;
    if (entry.isFile && entry.file) {
      const file = await new Promise<File>((ok, fail) => entry.file!(ok, fail));
      if (file.size <= MAX_FILE_SIZE)
        files.push({ path: entry.fullPath.replace(/^\//, ""), text: await file.text() });
    } else if (entry.isDirectory && entry.createReader) {
      const reader = entry.createReader();
      // readEntries hands back a batch at a time until it returns none.
      for (;;) {
        const batch = await new Promise<Entry[]>((ok, fail) => reader.readEntries(ok, fail));
        if (batch.length === 0) break;
        for (const child of batch) await visit(child);
      }
    }
  };
  for (const entry of entries) await visit(entry);
  return files;
}

/** Whether a drag carries files from outside the page. */
export function carriesFiles(data: DataTransfer | null): boolean {
  return !!data && Array.from(data.types).includes("Files");
}

/** Offers text to the browser as a file to save. */
export function download(fileName: string, content: string, type = "application/json"): void {
  const url = URL.createObjectURL(new Blob([content], { type }));
  const link = document.createElement("a");
  link.href = url;
  link.download = fileName;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
