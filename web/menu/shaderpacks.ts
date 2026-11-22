// The browser's memory for the game's own files: shader packs in `shaderpacks/`, their
// option files and `horizon-shaders.txt`. The wasm game keeps its files in memory
// (`horizon_core::fs`); this store mirrors the ones it wants kept into the origin private
// file system (OPFS) under `horizon-files/`, by the same relative path, and puts them
// back with `install_file` on the next visit: at boot only the selection, the option
// files and the selected pack (when shaders are on); every other pack is an empty
// placeholder until the game asks for it (`horizonLoadShaderPack`). It also gives the game the page-side
// helpers its shader pack screen calls: a file picker, drag and drop, and the clipboard.

/** The OPFS folder that mirrors the game folder. */
export const ROOT = "horizon-files";
/** The game's shader pack folder, relative to the game folder. */
export const PACKS = "shaderpacks";

export type InstallFile = (path: string, bytes: Uint8Array) => void;

/** The selection file the game keeps beside its options. */
export const SELECTION = "horizon-shaders.txt";
/** The largest zip the page takes (the game also refuses packs that unpack past 256 MiB). */
export const MAX_PACK_BYTES = 128 << 20;

/** A file name safe to use as one path segment: no slashes, no `..`, no control
 * characters. Spaces, `§`, `+`, brackets and the like stay, as packs use them. */
export function sanitizeName(name: string, fallback = "pack.zip"): string {
  const clean = name
    .replace(/[\u0000-\u001f\u007f]/g, "")
    .replace(/[/\\]/g, "_")
    .replace(/\.{2,}/g, ".")
    .trim();
  return clean === "" || clean === "." ? fallback : clean;
}

/** The segments of a relative game path; throws on an empty segment, `.` or `..`, so a
 * path can never leave `horizon-files/`. A leading `./` or `/` is dropped. */
export function splitPath(path: string): string[] {
  const parts = path.replace(/\\/g, "/").replace(/^(\.\/|\/)+/, "").split("/");
  for (const part of parts) {
    if (part === "" || part === "." || part === "..") throw new Error(`Bad game path: ${JSON.stringify(path)}`);
  }
  return parts;
}

/** Joins relative path segments with `/`, skipping empty ones. */
export function joinPath(...parts: string[]): string {
  return parts.filter(Boolean).join("/");
}

/** Where a chosen or dropped pack named `name` goes in the game folder. */
export function packPath(name: string): string {
  return joinPath(PACKS, sanitizeName(name));
}

/** The file name for a `?shaderpack=` URL: its path's last segment, sanitized. */
export function packNameFromUrl(url: string, base = "http://localhost/"): string {
  let last = "";
  try {
    last = new URL(url, base).pathname.split("/").pop() ?? "";
    last = decodeURIComponent(last);
  } catch {
    // An unparsable URL or escape: fall back to the default name.
  }
  return sanitizeName(last);
}

/** Whether a file looks like a shader pack zip, by name or type. */
export function isZip(file: { name: string; type?: string }): boolean {
  return /\.zip$/i.test(file.name) || file.type === "application/zip" || file.type === "application/x-zip-compressed";
}

async function root(): Promise<FileSystemDirectoryHandle> {
  const storage = navigator.storage as StorageManager | undefined;
  if (!storage?.getDirectory) throw new Error("This browser has no origin private file system");
  return (await storage.getDirectory()).getDirectoryHandle(ROOT, { create: true });
}

async function folder(parts: string[], create: boolean): Promise<FileSystemDirectoryHandle> {
  let dir = await root();
  for (const part of parts) dir = await dir.getDirectoryHandle(part, { create });
  return dir;
}

const isNotFound = (error: unknown) => (error as DOMException)?.name === "NotFoundError";

/** Every stored file's relative path, sorted. */
export async function listFiles(): Promise<string[]> {
  const found: string[] = [];
  const walk = async (dir: FileSystemDirectoryHandle, prefix: string) => {
    // `entries()` is in every OPFS browser; the DOM types this repo uses lack it.
    const entries = (dir as unknown as { entries(): AsyncIterable<[string, FileSystemHandle]> }).entries();
    for await (const [name, handle] of entries) {
      const path = joinPath(prefix, name);
      if (handle.kind === "directory") await walk(handle as FileSystemDirectoryHandle, path);
      else found.push(path);
    }
  };
  await walk(await root(), "");
  return found.sort();
}

/** A stored file's bytes, or undefined when there is none. */
export async function readFile(path: string): Promise<Uint8Array | undefined> {
  const parts = splitPath(path);
  try {
    const dir = await folder(parts.slice(0, -1), false);
    const file = await (await dir.getFileHandle(parts[parts.length - 1])).getFile();
    return new Uint8Array(await file.arrayBuffer());
  } catch (error) {
    if (isNotFound(error) || (error as DOMException)?.name === "TypeMismatchError") return undefined;
    throw error;
  }
}

async function write(parts: string[], bytes: Uint8Array) {
  const dir = await folder(parts.slice(0, -1), true);
  const writable = await (await dir.getFileHandle(parts[parts.length - 1], { create: true })).createWritable();
  try {
    await writable.write(bytes as Uint8Array<ArrayBuffer>);
    await writable.close();
  } catch (error) {
    await writable.abort().catch(() => {});
    throw error;
  }
}

async function remove(parts: string[]) {
  try {
    const dir = await folder(parts.slice(0, -1), false);
    await dir.removeEntry(parts[parts.length - 1]);
  } catch (error) {
    if (!isNotFound(error)) throw error;
  }
}

/** Writes and deletes run one at a time, in the order they were asked for, so a later
 * write to a path wins over an earlier one. */
let queue: Promise<void> = Promise.resolve();
function enqueue(job: () => Promise<void>): Promise<void> {
  const run = queue.then(job);
  queue = run.catch(() => {});
  return run;
}

/** Stores `bytes` at `path` (copied at once: the caller may reuse its memory). */
export function writeFile(path: string, bytes: Uint8Array): Promise<void> {
  const parts = splitPath(path), copy = bytes.slice();
  return enqueue(() => write(parts, copy));
}

/** Removes the file at `path`; a missing file is fine. Empty folders stay. */
export function deleteFile(path: string): Promise<void> {
  const parts = splitPath(path);
  return enqueue(() => remove(parts));
}

/** Resolves once every write and delete asked for so far has been applied. */
export function flush(): Promise<void> {
  return queue;
}

/** The game's `persist`: fire and forget; a failure logs and goes to `onError` (the
 * game's chat). */
export function persist(path: string, bytes: Uint8Array | null, onError?: (line: string) => void): void {
  const failed = (error: unknown) => {
    console.warn(`Keeping ${path} failed`, error);
    onError?.(`${path} could not be kept in this browser (${message(error)}): it will be gone after a reload.`);
  };
  try {
    const done = bytes ? writeFile(path, bytes) : deleteFile(path);
    done.catch(failed);
  } catch (error) {
    failed(error);
  }
}

const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** The pack `horizon-shaders.txt` turns on (`enabled:true` and its `pack:`), if any. */
export function selectedPack(text: string): string | undefined {
  let enabled = false, pack: string | undefined;
  for (const line of text.split(/\r?\n/)) {
    const at = line.indexOf(":");
    if (at < 0) continue;
    const key = line.slice(0, at), value = line.slice(at + 1).trim();
    if (key === "enabled") enabled = value === "true";
    else if (key === "pack" && value) pack = value;
  }
  return enabled ? pack : undefined;
}

/** Whether a stored path is a pack zip in the packs folder. */
const isStoredPack = (path: string) => path.startsWith(`${PACKS}/`) && path.split("/").length === 2 && /\.zip$/i.test(path);

/** Which stored files go into the game at boot (`now`) and which only as empty
 * placeholders, loaded when the game asks (`later`): every pack but the selected one. */
export function bootPlan(paths: string[], selected?: string): { now: string[]; later: string[] } {
  const now: string[] = [], later: string[] = [];
  const chosen = selected ? joinPath(PACKS, selected) : undefined;
  for (const path of paths) (isStoredPack(path) && path !== chosen ? later : now).push(path);
  return { now, later };
}

/** Boot: the selection, the option files and the selected pack go into the game, the
 * other packs as empty placeholders; returns how many files were read. Never throws. */
export async function mirrorAtBoot(installFile: InstallFile, paths: string[]): Promise<number> {
  let selected: string | undefined;
  if (paths.includes(SELECTION)) {
    try {
      const text = await readFile(SELECTION);
      if (text) selected = selectedPack(new TextDecoder().decode(text));
    } catch (error) {
      console.warn(`Loading the saved ${SELECTION} failed`, error);
    }
  }
  const { now, later } = bootPlan(paths, selected);
  for (const path of later) installFile(path, new Uint8Array(0));
  return mirrorInto(installFile, now);
}

/** Puts every stored file (or the `paths` given) into the game; returns how many. Never
 * throws: a browser without OPFS, or a full one, just starts with nothing. */
export async function mirrorInto(installFile: InstallFile, paths?: string[]): Promise<number> {
  let installed = 0;
  try {
    for (const path of paths ?? await listFiles()) {
      try {
        const bytes = await readFile(path);
        if (bytes) { installFile(path, bytes); installed++; }
      } catch (error) {
        console.warn(`Loading the saved ${path} failed`, error);
      }
    }
  } catch (error) {
    console.warn("Loading your saved shader packs failed", error);
  }
  return installed;
}

/** Asks the browser not to evict the store under storage pressure (once; the answer is
 * not needed). */
let persisted = false;
export function requestPersistence(): void {
  if (persisted) return;
  persisted = true;
  try {
    void navigator.storage?.persist?.().catch(() => {});
  } catch {
    // Not offered: the store is just best effort.
  }
}

/** Copies `text` to the clipboard, falling back to a hidden textarea; failures only log. */
export function copyText(text: string): void {
  const fallback = () => {
    try {
      const area = document.createElement("textarea");
      area.value = text;
      area.setAttribute("readonly", "");
      area.style.cssText = "position:fixed;left:-9999px;top:0;opacity:0";
      document.body.append(area);
      area.select();
      const ok = document.execCommand("copy");
      area.remove();
      if (!ok) console.warn("Copying to the clipboard failed");
    } catch (error) {
      console.warn("Copying to the clipboard failed", error);
    }
  };
  if (navigator.clipboard?.writeText) navigator.clipboard.writeText(text).catch(fallback);
  else fallback();
}

export interface Hooks {
  installFile: InstallFile;
  /** Called once after one or more packs were added (or one was loaded). */
  onChanged?: () => void;
  /** A line for the player (the game's chat): a pack that could not be stored or loaded. */
  notice?: (line: string) => void;
  /** Whether a drop on the page is a shader pack (the game's Shader Packs page shows). */
  acceptsDrop?: () => boolean;
}

type HookWindow = typeof globalThis & {
  horizonPersist?: (path: string, bytes: Uint8Array | null) => void;
  horizonPickShaderPack?: () => void;
  horizonCopyText?: (text: string) => void;
  horizonLoadShaderPack?: (path: string) => void;
};

/** The hidden picker's id (tests reach it by this). */
export const PICKER_ID = "horizon-shaderpack-input";

/** Installs the game's page hooks (`window.horizonPersist`, `horizonPickShaderPack`,
 * `horizonCopyText`) and drag and drop of zips on the page. Returns `addFiles` (what a
 * pick or drop runs; resolves once the packs are stored) and `dispose`. */
export function installHooks({ installFile, onChanged, notice, acceptsDrop }: Hooks) {
  const page = globalThis as HookWindow;
  const say = (line: string) => {
    try { notice?.(line); } catch (error) { console.warn(line, error); }
  };
  const changed = () => {
    try { onChanged?.(); } catch (error) { console.warn("The game's shader pack rescan failed", error); }
  };

  const addFiles = async (files: Iterable<File>) => {
    let added = 0;
    for (const file of files) {
      if (!isZip(file)) { say(`Not added: ${file.name} is not a .zip file.`); continue; }
      if (file.size > MAX_PACK_BYTES) {
        say(`Not added: ${file.name} is larger than ${MAX_PACK_BYTES >> 20} MB.`);
        continue;
      }
      const path = packPath(file.name);
      let bytes: Uint8Array;
      try {
        bytes = new Uint8Array(await file.arrayBuffer());
      } catch (error) {
        console.warn(`Reading ${file.name} failed`, error);
        continue;
      }
      installFile(path, bytes);
      added++;
      await writeFile(path, bytes).then(() => say(`Added shader pack ${sanitizeName(file.name)}.`), (error: unknown) => {
        console.warn(`Keeping ${path} failed`, error);
        say(`${file.name} could not be stored in this browser (${message(error)}): it will be gone after a reload.`);
      });
    }
    if (added) changed();
    return added;
  };

  /** A pack left out at boot, when the game first needs it. */
  const load = (path: string) => {
    void (async () => {
      try {
        const bytes = await readFile(path);
        if (!bytes?.length) throw new Error("it is not stored");
        installFile(path, bytes);
      } catch (error) {
        console.warn(`Loading the saved ${path} failed`, error);
        say(`Shader pack ${path} could not be loaded (${message(error)}).`);
        return;
      }
      changed();
    })();
  };

  const pick = () => {
    let input = document.getElementById(PICKER_ID) as HTMLInputElement | null;
    if (!input) {
      input = document.createElement("input");
      input.id = PICKER_ID;
      input.type = "file";
      input.accept = ".zip,application/zip";
      input.multiple = true;
      input.hidden = true;
      input.addEventListener("change", () => {
        const chosen = [...(input!.files ?? [])];
        input!.value = "";
        void addFiles(chosen);
      });
      document.body.append(input);
    }
    input.click();
  };

  const hasFiles = (event: DragEvent) => !!event.dataTransfer && [...event.dataTransfer.types].includes("Files");
  const dragover = (event: DragEvent) => {
    if (!hasFiles(event)) return;
    event.preventDefault();
    event.dataTransfer!.dropEffect = "copy";
  };
  const drop = (event: DragEvent) => {
    if (!hasFiles(event)) return;
    // Never opened by the browser; taken only on the game's Shader Packs page.
    event.preventDefault();
    if (acceptsDrop && !acceptsDrop()) {
      say("Not added: drop shader packs on Options > Horizon Settings > Shader Packs.");
      return;
    }
    void addFiles([...event.dataTransfer!.files]);
  };
  addEventListener("dragover", dragover);
  addEventListener("drop", drop);

  page.horizonPersist = (path, bytes) => persist(path, bytes, say);
  page.horizonPickShaderPack = pick;
  page.horizonCopyText = copyText;
  page.horizonLoadShaderPack = load;

  return {
    addFiles,
    dispose() {
      removeEventListener("dragover", dragover);
      removeEventListener("drop", drop);
      delete page.horizonPersist;
      delete page.horizonPickShaderPack;
      delete page.horizonCopyText;
      delete page.horizonLoadShaderPack;
    },
  };
}
