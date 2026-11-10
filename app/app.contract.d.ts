// @generated from Contract source signatures; do not edit.
// Static types do not replace runtime shape, identity, or grant checks.
//
// What app.ts writes, in brief (the declarations follow): `sources: Sources` has one
// function per source, given its arguments as an array (then store, storage, native) and
// answering its shape, or a Promise of it.
//   computerControl: (args: [string, string, string]) => Reply
//   computerInput: (args: [string, string, number, number, number, number, number, string]) => Reply
//   createBot: (args: [string, string, string, string]) => Reply
//   createGroup: (args: [string, string, string, string, string]) => Made
//   createSubagent: (args: [string, string, string, string, string, boolean]) => Reply
//   deleteBot: (args: [string]) => Reply
//   deleteGroup: (args: [string]) => Reply
//   deleteLogin: (args: [string]) => Reply
//   dropVoice: (args: [string, string]) => Reply
//   hideBot: (args: [string, boolean]) => Reply
//   loadAbout: (args: []) => About
//   loadBots: (args: [string]) => Bot[]
//   loadChat: (args: [string, string]) => Chat
//   loadComputer: (args: [string, string, string, string]) => Computer
//   loadConvos: (args: [string, string, string]) => Convo[]
//   loadDrafts: (args: []) => DraftPack
//   loadLineage: (args: [string, string, string]) => Lineage
//   loadModels: (args: [string, string, string]) => ModelList
//   loadPrefs: (args: []) => Prefs
//   loadProviders: (args: []) => Provider[]
//   loadRoom: (args: [string]) => Room
//   loadRoutines: (args: [string, string, string]) => RoutineList
//   loadSession: (args: [string]) => Session
//   loadShelf: (args: [string, string]) => Shelf
//   loadSuggestions: (args: []) => Suggestion[]
//   loadUsage: (args: [string, string]) => Usage
//   loadUsageReport: (args: [string, string, string, string]) => UsageReport
//   loadVault: (args: []) => Login[]
//   makeMainBot: (args: [string]) => Reply
//   markSeen: (args: [string]) => Reply
//   pinBot: (args: [string, boolean]) => Reply
//   pinMessage: (args: [string, string, string, string]) => Reply
//   reactTo: (args: [string, string, string]) => Reply
//   recolourBot: (args: [string, string]) => Reply
//   renameBot: (args: [string, string]) => Reply
//   reshapeBot: (args: [string, string]) => Reply
//   respondRequest: (args: [string, string, string, string, string, string]) => Reply
//   revealPassword: (args: [string]) => Revealed
//   saveDrafts: (args: [string[], string[], string[]]) => Reply
//   saveLogin: (args: [string, string, string, string, string, string]) => Reply
//   saveSubagent: (args: [string, string, string, string, string, boolean]) => Reply
//   sendMessage: (args: [string, string, string, string, string, string]) => Reply
//   sendRoom: (args: [string, string]) => Reply
//   sendVoice: (args: [string, number, string, string]) => Reply
//   setDefaultModel: (args: [string, string, string]) => Reply
//   setModel: (args: [string, string, string, string]) => Reply
//   setPassword: (args: [string]) => Reply
//   setTheme: (args: [string]) => Reply
//   setUserName: (args: [string]) => Reply
//   signIn: (args: [string]) => Reply
//   signOut: (args: []) => Reply
//   stageVoice: (args: [string, string, string, number]) => VoiceHeld
//   stopRun: (args: [string, string]) => Reply
//   updateGroup: (args: [string, string, string]) => Reply
//   uploadAttachment: (args: [string, string, string, string, string]) => Attached
// The shapes. None is exported: name one by its source, `type Reply = Result<'computerControl'>`.
//   Reply = { ok: boolean; error: string }
//   Session = { authenticated: boolean; passwordRequired: boolean }
//   Bot = { slug: string; name: string; label: string; line: string; time: string; initials: string; color: string; shape: string; main: boolean; pinned: boolean; hidden: boolean; ready: boolean; working: boolean; unread: boolean; parentSlug: string; prompt: string; inheritParentPrompt: boolean }
//   FileRef = { id: string; name: string; kind: string; size: string; url: string }
//   Choice = { label: string; value: string; key: string; warn: string }
//   Row = { id: string; kind: string; mine: boolean; text: string; title: string; time: string; head: boolean; tail: boolean; variant: string; files: FileRef[]; botSlug: string; botName: string; botColor: string; botShape: string; choices: Choice[]; placeholder: string; key: string; react: string; quote: string; tokens: string }
//   Suggestion = { name: string; about: string; color: string; shape: string; first: string }
//   About = { ok: boolean; version: string; harness: string; connected: boolean }
//   Pin = { id: string; text: string; missing: boolean }
//   Chat = { slug: string; rows: Row[]; working: boolean; partial: boolean; readOnly: boolean; pins: Pin[]; threadId: string }
//   RoomMember = { slug: string; name: string; line: string; state: string; ready: boolean; working: boolean }
//   RoomMessage = { id: string; name: string; text: string; from: string }
//   Room = { id: string; title: string; description: string; color: string; shape: string; members: RoomMember[]; messages: RoomMessage[] }
//   FaceBit = { shape: string; color: string; state: string }
//   Convo = { kind: string; key: string; id: string; name: string; color: string; shape: string; faces: FaceBit[]; line: string; time: string; at: string; unread: boolean; working: boolean; ready: boolean }
//   Made = { ok: boolean; id: string; error: string }
//   Prefs = { userName: string; theme: string; defaultInstance: string; defaultModel: string }
//   Attached = { ok: boolean; id: string; name: string; error: string }
//   VoiceHeld = { ok: boolean; id: string; name: string; url: string; seconds: number; error: string }
//   Login = { id: string; title: string; website: string; username: string; hasPassword: boolean; hints: string; icon: string }
//   Revealed = { ok: boolean; id: string; password: string; error: string }
//   Usage = { ok: boolean; error: string; messages: number; attachments: number; vaultWrites: number; iconFetches: number }
//   Effort = { id: string; label: string }
//   ModelOption = { slug: string; name: string; recommended: boolean; effort: string; efforts: Effort[] }
//   Provider = { instanceId: string; name: string; account: string; models: ModelOption[] }
//   ModelList = { ok: boolean; error: string; current: string; instanceId: string; effort: string; currentName: string; providerName: string; effortLabel: string; efforts: Effort[]; providers: Provider[] }
//   Routine = { id: string; title: string; paused: boolean; schedule: string; next: string; failed: boolean }
//   RoutineList = { ok: boolean; tasks: Routine[]; error: string }
//   LineRow = { id: string; title: string; status: string; remote: boolean; threadId: string }
//   Lineage = { ok: boolean; rows: LineRow[]; error: string }
//   DayRow = { day: string; label: string; short: string; tokens: string; input: string; output: string; cached: string; cacheWrite: string; turns: string; messages: string; px: number; spx: number }
//   ModelRow = { name: string; tokens: string; turns: string; share: number }
//   BotShare = { slug: string; name: string; color: string; shape: string; tokens: string; turns: string; messages: string; share: number }
//   UsageReport = { ok: boolean; error: string; period: string; periodLabel: string; messages: string; attachments: string; turns: string; tokens: string; input: string; output: string; cached: string; cacheWrite: string; hit: string; sentence: string; days: DayRow[]; rows: DayRow[]; models: ModelRow[]; bots: BotShare[]; empty: boolean; first: string; last: string }
//   ShelfItem = { id: string; title: string; detail: string; url: string }
//   Shelf = { ok: boolean; total: number; pictures: ShelfItem[]; files: ShelfItem[]; links: ShelfItem[] }
//   PageTab = { id: string; title: string; url: string }
//   Computer = { ok: boolean; tabs: PageTab[]; image: string; error: string; selected: string }
//   DraftPack = { stamp: string; texts: string[]; files: string[]; voices: string[] }

/** Storage capabilities supplied by the host. Import these types; there are
 * no ambient fs/sqlite globals and importing a type conveys no authority. */
export interface Storage {
  readonly fs: FileSystem;
  readonly sqlite: SQLite;
}

export interface FileSystem {
  readonly directories: {
    readonly data: "app:/data";
    readonly cache: "app:/cache";
    readonly temporary: "app:/tmp";
  };
  readFile(path: string): Promise<ArrayBuffer>;
  writeFile(path: string, data: ArrayBuffer | Uint8Array, options?: WriteOptions): Promise<void>;
  /** Atomic replacement visibility; not a power-loss durability guarantee. */
  atomicWriteFile(path: string, data: ArrayBuffer | Uint8Array, options?: WriteOptions): Promise<void>;
  appendFile(path: string, data: ArrayBuffer | Uint8Array, options?: WriteOptions): Promise<void>;
  readdir(path: string): Promise<string[]>;
  mkdir(path: string): Promise<void>;
  rm(path: string): Promise<void>;
  stat(path: string): Promise<FileStat>;
  rename(from: string, to: string): Promise<void>;
  copyFile(from: string, to: string): Promise<void>;
  realpath(path: string): Promise<string>;
  /** Exact patch 24 (exact2 LLP 1133.007 D6a): `stat` of a link itself. */
  lstat(path: string): Promise<FileStat>;
  /** A symbolic link's target. Needs `fs.read`. */
  readlink(path: string): Promise<string>;
  /** Resolves when this user may access the path as `mode` asks (0 exists,
   * 1 execute, 2 write, 4 read), counting ownership, ACLs and mounts. */
  access(path: string, mode: number): Promise<void>;
  /** Its permission bits; on Windows only the read-only bit. Needs `fs.write`. */
  chmod(path: string, mode: number): Promise<void>;
  /** A symbolic link at `path` to `target`, as Node's. Needs `fs.write` on `path`. */
  symlink(target: string, path: string): Promise<void>;
  /** A file's SHA-256, -384 or -512, lowercase hex, read in bounded memory. */
  digest(path: string, algorithm: "SHA-256" | "SHA-384" | "SHA-512"): Promise<string>;
  /** An exclusive advisory lock (POSIX `flock`), made if absent and left on
   * disk, waiting up to `timeout` ms: held until `unlock()`. Needs `fs.write`. */
  flock(path: string, options?: { timeout?: number }): Promise<{ unlock(): Promise<void> }>;
  /** Exact patch 9 (exact2 LLP 1069.002 A1), the app interface: decode the
   * image at `from`, scale it to fit `maxDimension` and encode a JPEG of at
   * most `maxBytes` at `to`, searching quality as Bluesky's composer does;
   * or, with `type: "image/png"` (A1.11), a PNG that keeps alpha, shrunk by
   * 0.8 up to four times until it fits. Orientation is applied and no
   * metadata is written. Needs `fs.read` on
   * `from` and `fs.write` on `to`, both `app:/` paths. Rejects with a
   * `TypeError` for a bad option, else with `code` `unsupported` (no codec
   * on this host), `undecodable`, `too-large` (over 64 MiB, 64 Mi pixels,
   * or more decoding memory than the host allows), `unfit` (nothing fits) or
   * `timeout` (20 s), or a filesystem code. With `rotate`, `flip` or `crop`
   * (exact2 LLP 1133.019 D15) the edit is made first, in `transformImage`'s
   * order, and the search runs on the edited picture; a crop that misses it
   * rejects with `code` `outside`. */
  compressImage(from: string, to: string, options: CompressImageOptions): Promise<CompressedImage>;
  /** Exact patch 9a (exact2 LLP 1133.019 D14), the app interface: decode
   * the image at `from` once, turn it upright, then `rotate` it about its
   * centre (degrees clockwise; a free angle takes the bounding box, clear
   * where the type keeps alpha and white otherwise), `flip` it in the
   * turned frame, `crop` it in that frame's pixels, size it as
   * `createImageBitmap` does, and encode it once at `to`. Unlike
   * `compressImage` it keeps what the camera made where the host can: the
   * source's colour space, an HDR gain map (JPEG and HEIC on Apple) and,
   * by `metadata`, its EXIF, TIFF, GPS, IPTC and XMP; the result says what
   * was written. A `to` with no extension gains the type's. Needs `fs.read`
   * on `from` and `fs.write` on `to`, both `app:/` paths. Rejects with a
   * `TypeError` for a bad option, else with `code` `unsupported` (a type
   * this host does not write), `undecodable`, `too-large`, `outside` (the
   * crop misses the picture) or `timeout`, or a filesystem code. */
  transformImage(from: string, to: string, options?: TransformImageOptions): Promise<TransformedImage>;
  /** Exact patch 14 (exact2 LLP 1069.010.000 D1), the app interface: this
   * answer depends on the `doc:/` path, and the host asks it again when the
   * file, a folder's own entries or, with `recursive`, its subtree change on
   * disk, by any process. Call it inside an answer; it carries no value, so
   * the answer reads again. Needs `fs.read` on the path. */
  watch(path: string, options?: WatchOptions): void;
  /** Exact patch 15 (exact2 LLP 1069.010.000 D5), the app interface: the
   * documents the host keeps for this app across launches, most recent
   * first, without reading them. `watch("doc:/")` asks the answer again when
   * the list changes. Needs the `doc.keep` grant; a host that keeps nothing
   * lists nothing. */
  recentDocuments(): Promise<KeptDocument[]>;
  /** Exact patch 15: the app's own removal of a kept document (a "Remove
   * from Recents" item). Rejects with `code` `ENOENT` for a path it does not
   * keep. Needs the `doc.keep` grant. */
  forgetDocument(path: string): Promise<void>;
}

export interface KeptDocument {
  /** Its `doc:/<n>/<name>`, valid in every launch while it is kept. */
  readonly path: string;
  /** Its name when chosen. */
  readonly name: string;
  readonly kind: "file" | "directory";
  /** What the choice granted: `read`, or `readwrite`. */
  readonly mode: "read" | "readwrite";
  /** When it was last chosen or opened, in milliseconds since 1970. */
  readonly opened: number;
}

export interface WatchOptions {
  /** The whole subtree, not only a folder's own entries. */
  readonly recursive?: boolean;
}

export interface WriteOptions {
  /** Exact patch 14 (exact2 LLP 1069.010.000 D2a): on a `doc:/` path, the
   * stat last read. The write is refused with `code` `ECHANGED`, writing
   * nothing, when the file's `modifiedMs` or `size` is no longer this. */
  readonly expect?: Pick<FileStat, "modifiedMs" | "size">;
}

export interface CompressImageOptions {
  /** An integer from 1 to 8192: the longer side's limit, in pixels. */
  readonly maxDimension: number;
  /** An integer from 1 to 67108864: the encoded file's limit. */
  readonly maxBytes: number;
  /** What to write; `"image/jpeg"` when absent. */
  readonly type?: "image/jpeg" | "image/png";
  /** Exact patch 9a (exact2 LLP 1133.019 D15): `transformImage`'s edit. */
  readonly rotate?: number;
  readonly flip?: "horizontal" | "vertical";
  readonly crop?: ImageCrop;
}

/** A rectangle in the turned picture's pixels, as a `DOMRect` names one.
 * It is clipped to the picture and its edges rounded. */
export interface ImageCrop {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

export interface TransformImageOptions {
  /** Degrees clockwise, as CSS's `rotate` turns. */
  readonly rotate?: number;
  /** In the turned frame: `horizontal` mirrors what the person sees. */
  readonly flip?: "horizontal" | "vertical";
  readonly crop?: ImageCrop;
  /** `createImageBitmap`'s: one keeps the aspect, both set the size,
   * neither keeps the crop's. Integers from 1 to 16384. */
  readonly resizeWidth?: number;
  readonly resizeHeight?: number;
  /** `"high"` when absent. */
  readonly resizeQuality?: "pixelated" | "low" | "medium" | "high";
  /** The source's type when the host writes it, else `"image/jpeg"`. A
   * type the host does not write rejects with `code` `unsupported`. */
  readonly type?: string;
  /** From 0 to 1, for a lossy type; 0.9 when absent. */
  readonly quality?: number;
  /** What of the source's metadata to write: all of it, with the
   * orientation reset and the size rewritten (`"keep"`, when absent); the
   * capture's standard tags and the IPTC rights fields, with no GPS, maker
   * note or XMP (`"no-location"`); or none. */
  readonly metadata?: "keep" | "no-location" | "none";
}

export interface TransformedImage {
  /** `to`, with the type's extension where it had none. */
  readonly path: string;
  readonly type: string;
  readonly size: number;
  readonly width: number;
  readonly height: number;
  /** What was written, which a host that cannot keep something says. */
  readonly colorSpace: "srgb" | "display-p3" | "other";
  readonly hdr: boolean;
  readonly metadata: "keep" | "no-location" | "none";
}

export interface CompressedImage {
  readonly path: string;
  readonly type: "image/jpeg" | "image/png";
  readonly size: number;
  readonly width: number;
  readonly height: number;
}

export interface FileStat {
  readonly size: number;
  readonly isFile: boolean;
  readonly isDirectory: boolean;
  readonly modifiedMs: number;
  /** Exact patch 24: its permission bits, natively (the web's records have none). */
  readonly mode?: number;
  /** Exact patch 24: an `lstat` that met a symbolic link. */
  readonly isSymbolicLink?: boolean;
}

/** SQLite INTEGER results are bigint, including small integers. Number
 * inputs must be finite; integral numbers must be safely representable. */
export type SQLValue = null | bigint | number | string | Uint8Array;
export interface SQLRows {
  columns: string[];
  rows: SQLValue[][];
}
export interface SQLExecution {
  changes: number;
  lastInsertRowid: bigint;
}
export interface SQLCommand {
  sql: string;
  params?: SQLValue[];
}
export interface SQLite {
  open(path: string): Promise<Database>;
}
export interface Database {
  execute(sql: string, params?: SQLValue[]): Promise<SQLExecution>;
  /** Read-only queries; one SQL statement per call. */
  query(sql: string, params?: SQLValue[]): Promise<SQLRows>;
  prepare(sql: string): Promise<Statement>;
  /** One atomic batch, rolled back on failure; no callback transaction. */
  transaction(commands: SQLCommand[]): Promise<SQLExecution[]>;
  /** Idempotent; invalidates this database's prepared statements. */
  close(): Promise<void>;
}
export interface Statement {
  execute(params?: SQLValue[]): Promise<SQLExecution>;
  query(params?: SQLValue[]): Promise<SQLRows>;
  close(): Promise<void>;
}

type T0 = boolean;
type T1 = string;
type T2 = { "ok": T0; "error": T1; };
type T3 = { "authenticated": T0; "passwordRequired": T0; };
type T4 = { "slug": T1; "name": T1; "label": T1; "line": T1; "time": T1; "initials": T1; "color": T1; "shape": T1; "main": T0; "pinned": T0; "hidden": T0; "ready": T0; "working": T0; "unread": T0; "parentSlug": T1; "prompt": T1; "inheritParentPrompt": T0; };
type T5 = { "id": T1; "name": T1; "kind": T1; "size": T1; "url": T1; };
type T6 = { "label": T1; "value": T1; "key": T1; "warn": T1; };
type T7 = T5[];
type T8 = T6[];
type T9 = { "id": T1; "kind": T1; "mine": T0; "text": T1; "title": T1; "time": T1; "head": T0; "tail": T0; "variant": T1; "files": T7; "botSlug": T1; "botName": T1; "botColor": T1; "botShape": T1; "choices": T8; "placeholder": T1; "key": T1; "react": T1; "quote": T1; "tokens": T1; };
type T10 = { "name": T1; "about": T1; "color": T1; "shape": T1; "first": T1; };
type T11 = { "ok": T0; "version": T1; "harness": T1; "connected": T0; };
type T12 = { "id": T1; "text": T1; "missing": T0; };
type T13 = T9[];
type T14 = T12[];
type T15 = { "slug": T1; "rows": T13; "working": T0; "partial": T0; "readOnly": T0; "pins": T14; "threadId": T1; };
type T16 = { "slug": T1; "name": T1; "line": T1; "state": T1; "ready": T0; "working": T0; };
type T17 = { "id": T1; "name": T1; "text": T1; "from": T1; };
type T18 = T16[];
type T19 = T17[];
type T20 = { "id": T1; "title": T1; "description": T1; "color": T1; "shape": T1; "members": T18; "messages": T19; };
type T21 = { "shape": T1; "color": T1; "state": T1; };
type T22 = T21[];
type T23 = { "kind": T1; "key": T1; "id": T1; "name": T1; "color": T1; "shape": T1; "faces": T22; "line": T1; "time": T1; "at": T1; "unread": T0; "working": T0; "ready": T0; };
type T24 = { "ok": T0; "id": T1; "error": T1; };
type T25 = { "userName": T1; "theme": T1; "defaultInstance": T1; "defaultModel": T1; };
type T26 = { "ok": T0; "id": T1; "name": T1; "error": T1; };
type T27 = number;
type T28 = { "ok": T0; "id": T1; "name": T1; "url": T1; "seconds": T27; "error": T1; };
type T29 = { "id": T1; "title": T1; "website": T1; "username": T1; "hasPassword": T0; "hints": T1; "icon": T1; };
type T30 = { "ok": T0; "id": T1; "password": T1; "error": T1; };
type T31 = { "ok": T0; "error": T1; "messages": T27; "attachments": T27; "vaultWrites": T27; "iconFetches": T27; };
type T32 = { "id": T1; "label": T1; };
type T33 = T32[];
type T34 = { "slug": T1; "name": T1; "recommended": T0; "effort": T1; "efforts": T33; };
type T35 = T34[];
type T36 = { "instanceId": T1; "name": T1; "account": T1; "models": T35; };
type T37 = T36[];
type T38 = { "ok": T0; "error": T1; "current": T1; "instanceId": T1; "effort": T1; "currentName": T1; "providerName": T1; "effortLabel": T1; "efforts": T33; "providers": T37; };
type T39 = { "id": T1; "title": T1; "paused": T0; "schedule": T1; "next": T1; "failed": T0; };
type T40 = T39[];
type T41 = { "ok": T0; "tasks": T40; "error": T1; };
type T42 = { "id": T1; "title": T1; "status": T1; "remote": T0; "threadId": T1; };
type T43 = T42[];
type T44 = { "ok": T0; "rows": T43; "error": T1; };
type T45 = { "day": T1; "label": T1; "short": T1; "tokens": T1; "input": T1; "output": T1; "cached": T1; "cacheWrite": T1; "turns": T1; "messages": T1; "px": T27; "spx": T27; };
type T46 = { "name": T1; "tokens": T1; "turns": T1; "share": T27; };
type T47 = { "slug": T1; "name": T1; "color": T1; "shape": T1; "tokens": T1; "turns": T1; "messages": T1; "share": T27; };
type T48 = T45[];
type T49 = T46[];
type T50 = T47[];
type T51 = { "ok": T0; "error": T1; "period": T1; "periodLabel": T1; "messages": T1; "attachments": T1; "turns": T1; "tokens": T1; "input": T1; "output": T1; "cached": T1; "cacheWrite": T1; "hit": T1; "sentence": T1; "days": T48; "rows": T48; "models": T49; "bots": T50; "empty": T0; "first": T1; "last": T1; };
type T52 = { "id": T1; "title": T1; "detail": T1; "url": T1; };
type T53 = T52[];
type T54 = { "ok": T0; "total": T27; "pictures": T53; "files": T53; "links": T53; };
type T55 = { "id": T1; "title": T1; "url": T1; };
type T56 = T55[];
type T57 = { "ok": T0; "tabs": T56; "image": T1; "error": T1; "selected": T1; };
type T58 = T1[];
type T59 = { "stamp": T1; "texts": T58; "files": T58; "voices": T58; };
type T60 = { "locale": T1; };
type T61 = { "hover": T1; };
type T62 = { "accessibilityTextSize": T0; };
type T63 = { "prefersContrast": T1; };
type T64 = { "permanentlyDenied": T0; "shouldShowRationale": T0; };
type T65 = T64 | null;
type T66 = "notDetermined" | "restricted" | "denied" | "authorized";
type T67 = { "authorizationStatus": T66; };
type T68 = T67 | null;
type T69 = "granted" | "prompt" | "denied" | "unknown";
type T70 = { "android": T65; "apple": T68; "available": T0; "state": T69; };
type T71 = T27 | null;
type T72 = { "max": T27; "min": T27; "step": T71; };
type T73 = T1 | null;
type T74 = { "refused": T73; };
type T75 = "audioinput" | "videoinput" | "audiooutput";
type T76 = { "deviceId": T1; "facingMode": T73; "groupId": T1; "kind": T75; "label": T1; };
type T77 = T76[];
type T78 = { "devices": T77; };
type T79 = { "cameraId": T73; "isClientSilenced": T0; "lensFacing": T73; };
type T80 = T79 | null;
type T81 = { "deviceType": T73; "portName": T73; "portType": T73; "position": T73; };
type T82 = T81 | null;
type T83 = T72 | null;
type T84 = T58 | null;
type T85 = T0 | null;
type T86 = { "aspectRatio": T83; "facingMode": T84; "frameRate": T83; "height": T83; "torch": T85; "width": T83; "zoom": T83; };
type T87 = T86 | null;
type T88 = "audio" | "video";
type T89 = "idle" | "interrupted" | "background" | "silenced" | "in-use" | "multitasking" | "system-pressure";
type T90 = T89 | null;
type T91 = "live" | "ended";
type T92 = { "aspectRatio": T71; "autoGainControl": T85; "channelCount": T71; "deviceId": T73; "echoCancellation": T85; "facingMode": T73; "frameRate": T71; "groupId": T73; "height": T71; "latency": T71; "noiseSuppression": T85; "resizeMode": T73; "sampleRate": T71; "sampleSize": T71; "torch": T85; "width": T71; "zoom": T71; };
type T93 = { "android": T80; "apple": T82; "capabilities": T87; "enabled": T0; "id": T1; "kind": T88; "label": T1; "muteReason": T90; "muted": T0; "readyState": T91; "settings": T92; };
type T94 = T93[];
type T95 = { "active": T0; "id": T1; "tracks": T94; };
type T96 = { "aspectRatio": T71; "autoGainControl": T85; "channelCount": T71; "deviceId": T73; "echoCancellation": T85; "facingMode": T73; "frameRate": T71; "groupId": T73; "height": T71; "latency": T71; "noiseSuppression": T85; "resizeMode": T73; "sampleRate": T71; "sampleSize": T71; "torch": T85; "width": T71; "zoom": T71; };
type T97 = { "permanentlyDenied": T0; "shouldShowRationale": T0; };
type T98 = T97 | null;
type T99 = "undetermined" | "denied" | "granted" | "notDetermined" | "restricted" | "authorized";
type T100 = { "recordPermission": T99; };
type T101 = T100 | null;
type T102 = "granted" | "prompt" | "denied" | "unknown";
type T103 = { "android": T98; "apple": T101; "available": T0; "state": T102; };
type T104 = { "x": T27; "y": T27; };
type T105 = T27[];
type T106 = { "audioLevel": T27; "levels": T105; "totalAudioEnergy": T27; "totalSamplesDuration": T27; };
type T107 = "requested" | "interrupted" | "media-reset" | "background" | "track-ended" | "error" | "session-ended";
type T108 = T107 | null;
type T109 = { "duration": T27; "mimeType": T1; "path": T1; "size": T27; };
type T110 = T109 | null;
type T111 = "inactive" | "recording" | "paused";
type T112 = { "audioBitsPerSecond": T71; "duration": T27; "error": T73; "id": T1; "mimeType": T1; "path": T1; "reason": T108; "result": T110; "state": T111; };
type T113 = { "supported": T0; "type": T1; };
type T114 = { "body": T73; "data": T73; "tag": T1; "title": T1; };
type T115 = "notSupported" | "disabled" | "enabled";
type T116 = "notDetermined" | "denied" | "authorized" | "provisional" | "ephemeral";
type T117 = "notSupported" | "disabled" | "enabled";
type T118 = "notSupported" | "disabled" | "enabled";
type T119 = { "alertSetting": T115; "authorizationStatus": T116; "badgeSetting": T117; "soundSetting": T118; };
type T120 = T119 | null;
type T121 = "granted" | "prompt" | "denied" | "unknown";
type T122 = { "apple": T120; "available": T0; "badge": T0; "state": T121; };
type T123 = "mic" | "camcorder" | "voiceRecognition" | "voiceCommunication" | "unprocessed" | "voicePerformance";
type T124 = T123 | null;
type T125 = { "audioSource": T124; "privacySensitive": T85; };
type T126 = "mixWithOthers" | "duckOthers" | "allowBluetoothHFP" | "allowBluetoothA2DP" | "defaultToSpeaker" | "bluetoothHighQualityRecording" | "farFieldInput" | "overrideMutedMicrophoneInterruption";
type T127 = T126[];
type T128 = T127 | null;
type T129 = "builtInWideAngleCamera" | "builtInUltraWideCamera" | "builtInTelephotoCamera" | "builtInDualCamera" | "builtInDualWideCamera" | "builtInTripleCamera" | "builtInTrueDepthCamera" | "builtInLiDARDepthCamera" | "continuityCamera" | "deskViewCamera" | "external";
type T130 = T129 | null;
type T131 = "default" | "measurement" | "voiceChat" | "videoChat" | "videoRecording";
type T132 = T131 | null;
type T133 = "stereo" | "cardioid" | "omnidirectional";
type T134 = T133 | null;
type T135 = { "categoryOptions": T128; "deviceType": T130; "inputDataSource": T73; "mode": T132; "polarPattern": T134; "preferredIOBufferDuration": T71; "prefersNoInterruptionsFromSystemAlerts": T85; };
type T136 = T96 | null;
type T137 = T125 | null;
type T138 = T135 | null;
type T139 = { "audio": T85; "audioConstraints": T136; "video": T85; "videoConstraints": T136; "android": T137; "apple": T138; };
type T140 = "builtInWideAngleCamera" | "builtInUltraWideCamera" | "builtInTelephotoCamera" | "builtInDualCamera" | "builtInDualWideCamera" | "builtInTripleCamera" | "builtInTrueDepthCamera" | "builtInLiDARDepthCamera" | "continuityCamera" | "deskViewCamera" | "external";
type T141 = T140 | null;
type T142 = "locked" | "autoExpose" | "continuousAutoExposure";
type T143 = T142 | null;
type T144 = T104 | null;
type T145 = "locked" | "autoFocus" | "continuousAutoFocus";
type T146 = T145 | null;
type T147 = { "deviceType": T141; "exposureMode": T143; "exposurePointOfInterest": T144; "focusMode": T146; "focusPointOfInterest": T144; };
type T148 = T147 | null;
type T149 = { "aspectRatio": T71; "facingMode": T73; "frameRate": T71; "height": T71; "resizeMode": T73; "torch": T85; "track": T1; "width": T71; "zoom": T71; "apple": T148; };
type T150 = { "track": T1; };
type T151 = { "enabled": T0; "track": T1; };
type T152 = { "track": T1; };
type T153 = "aac" | "heAac" | "opus";
type T154 = T153 | null;
type T155 = "mpeg4" | "ogg" | "webm";
type T156 = T155 | null;
type T157 = { "audioEncoder": T154; "outputFormat": T156; };
type T158 = "min" | "low" | "medium" | "high" | "max";
type T159 = T158 | null;
type T160 = "constant" | "longTermAverage" | "variableConstrained" | "variable";
type T161 = T160 | null;
type T162 = "aac" | "alac" | "linearPCM" | "opus";
type T163 = T162 | null;
type T164 = { "audioQuality": T159; "bitRateStrategy": T161; "formatID": T163; };
type T165 = "constant" | "variable";
type T166 = T165 | null;
type T167 = T157 | null;
type T168 = T164 | null;
type T169 = { "audioBitrateMode": T166; "audioBitsPerSecond": T71; "mimeType": T73; "path": T1; "stream": T1; "android": T167; "apple": T168; };
type T170 = { "recorder": T1; };
type T171 = { "recorder": T1; };
type T172 = { "recorder": T1; };
type T173 = { "interval": T71; "recorder": T1; };
type T174 = { "type": T1; };
type T175 = { "history": T71; "interval": T71; "track": T1; };
type T176 = "passive" | "active" | "timeSensitive";
type T177 = T176 | null;
type T178 = { "interruptionLevel": T177; "relevanceScore": T71; "subtitle": T73; "targetContentIdentifier": T73; "threadIdentifier": T73; };
type T179 = T178 | null;
type T180 = { "body": T73; "data": T73; "navigate": T73; "showTrigger": T71; "silent": T85; "tag": T73; "title": T1; "apple": T179; };
type T181 = { "tag": T1; };
type T182 = { "contents": T71; };
type T183 = { "slug": T1; "id": T1; };
type T184 = { "id": T27; "name": T1; "url": T1; "tab": T1; "params": T183; "fragment": T1; };
type T185 = T184[];
type T186 = { "name": T1; "stack": T185; };
type T187 = T186[];
type T188 = { "tab": T1; "tabs": T187; "next": T27; };
type T189 = T4[];
type T190 = T29[];
type T191 = T23[];
type T192 = T10[];
type T193 = null;
type T194 = T2 | null;
type T195 = T24 | null;
type T196 = T122 | null;
type T197 = T30 | null;
type T198 = T26 | null;
type T199 = T95 | null;
type T200 = T112 | null;
type T201 = T93 | null;
type T202 = T28 | null;
type T203 = { "key": T1; "shiftKey": T0; "ctrlKey": T0; "altKey": T0; "metaKey": T0; "code": T1; "repeat": T0; "isComposing": T0; };
type T204 = { "path": T1; "name": T1; "type": T1; "size": T27; "width": T71; "height": T71; "duration": T71; };
type T205 = T204[];
type T206 = { "offsetX": T27; "offsetY": T27; "files": T58; "shiftKey": T0; "ctrlKey": T0; "altKey": T0; "metaKey": T0; };
type T207 = { "scrollLeft": T27; "scrollTop": T27; "scrollWidth": T27; "scrollHeight": T27; "clientWidth": T27; "clientHeight": T27; "firstVisibleKey": T73; "firstVisibleOffset": T27; };
type T208 = { "text": T1; "files": T58; };
type T209 = { "offsetX": T27; "offsetY": T27; "button": T27; "buttons": T27; "pressure": T27; "pointerType": T1; "pointerId": T27; "clientX": T27; "clientY": T27; "shiftKey": T0; "ctrlKey": T0; "altKey": T0; "metaKey": T0; };
type T210 = { "offsetX": T27; "offsetY": T27; "deltaX": T27; "deltaY": T27; "deltaMode": T27; "shiftKey": T0; "ctrlKey": T0; "altKey": T0; "metaKey": T0; };
type T211 = { "target": T1; "relatedTarget": T73; };

/** Sources called by compiled mutation sends. */
export type MutationSource = "computerControl" | "computerInput" | "createBot" | "createGroup" | "createSubagent" | "deleteBot" | "deleteGroup" | "deleteLogin" | "dropVoice" | "hideBot" | "makeMainBot" | "markSeen" | "pinBot" | "pinMessage" | "reactTo" | "recolourBot" | "renameBot" | "reshapeBot" | "respondRequest" | "revealPassword" | "saveDrafts" | "saveLogin" | "saveSubagent" | "sendMessage" | "sendRoom" | "sendVoice" | "setDefaultModel" | "setModel" | "setPassword" | "setTheme" | "setUserName" | "signIn" | "signOut" | "stageVoice" | "stopRun" | "updateGroup" | "uploadAttachment";

export interface SourceMap {
  "computerControl": { args: [T1, T1, T1]; result: T2 };
  "computerInput": { args: [T1, T1, T27, T27, T27, T27, T27, T1]; result: T2 };
  "createBot": { args: [T1, T1, T1, T1]; result: T2 };
  "createGroup": { args: [T1, T1, T1, T1, T1]; result: T24 };
  "createSubagent": { args: [T1, T1, T1, T1, T1, T0]; result: T2 };
  "deleteBot": { args: [T1]; result: T2 };
  "deleteGroup": { args: [T1]; result: T2 };
  "deleteLogin": { args: [T1]; result: T2 };
  "dropVoice": { args: [T1, T1]; result: T2 };
  "hideBot": { args: [T1, T0]; result: T2 };
  "loadAbout": { args: []; result: T11 };
  "loadBots": { args: [T1]; result: T189 };
  "loadChat": { args: [T1, T1]; result: T15 };
  "loadComputer": { args: [T1, T1, T1, T1]; result: T57 };
  "loadConvos": { args: [T1, T1, T1]; result: T191 };
  "loadDrafts": { args: []; result: T59 };
  "loadLineage": { args: [T1, T1, T1]; result: T44 };
  "loadModels": { args: [T1, T1, T1]; result: T38 };
  "loadPrefs": { args: []; result: T25 };
  "loadProviders": { args: []; result: T37 };
  "loadRoom": { args: [T1]; result: T20 };
  "loadRoutines": { args: [T1, T1, T1]; result: T41 };
  "loadSession": { args: [T1]; result: T3 };
  "loadShelf": { args: [T1, T1]; result: T54 };
  "loadSuggestions": { args: []; result: T192 };
  "loadUsage": { args: [T1, T1]; result: T31 };
  "loadUsageReport": { args: [T1, T1, T1, T1]; result: T51 };
  "loadVault": { args: []; result: T190 };
  "makeMainBot": { args: [T1]; result: T2 };
  "markSeen": { args: [T1]; result: T2 };
  "pinBot": { args: [T1, T0]; result: T2 };
  "pinMessage": { args: [T1, T1, T1, T1]; result: T2 };
  "reactTo": { args: [T1, T1, T1]; result: T2 };
  "recolourBot": { args: [T1, T1]; result: T2 };
  "renameBot": { args: [T1, T1]; result: T2 };
  "reshapeBot": { args: [T1, T1]; result: T2 };
  "respondRequest": { args: [T1, T1, T1, T1, T1, T1]; result: T2 };
  "revealPassword": { args: [T1]; result: T30 };
  "saveDrafts": { args: [T58, T58, T58]; result: T2 };
  "saveLogin": { args: [T1, T1, T1, T1, T1, T1]; result: T2 };
  "saveSubagent": { args: [T1, T1, T1, T1, T1, T0]; result: T2 };
  "sendMessage": { args: [T1, T1, T1, T1, T1, T1]; result: T2 };
  "sendRoom": { args: [T1, T1]; result: T2 };
  "sendVoice": { args: [T1, T27, T1, T1]; result: T2 };
  "setDefaultModel": { args: [T1, T1, T1]; result: T2 };
  "setModel": { args: [T1, T1, T1, T1]; result: T2 };
  "setPassword": { args: [T1]; result: T2 };
  "setTheme": { args: [T1]; result: T2 };
  "setUserName": { args: [T1]; result: T2 };
  "signIn": { args: [T1]; result: T2 };
  "signOut": { args: []; result: T2 };
  "stageVoice": { args: [T1, T1, T1, T27]; result: T28 };
  "stopRun": { args: [T1, T1]; result: T2 };
  "updateGroup": { args: [T1, T1, T1]; result: T2 };
  "uploadAttachment": { args: [T1, T1, T1, T1, T1]; result: T26 };
}

export type Source = keyof SourceMap;
export type Args<S extends Source> = SourceMap[S]['args'];
/** A generated backend operation's arguments (distinct from this app's source tuple). */
export type ArgsOf<O> = O extends { readonly __args?: infer A } ? A : never;
/** A source's answer. A shape has no exported name: name one by its source,
* `type Recipe = Result<'recipe'>` (a list's element: `Result<'recipes'>[number]`). */
export type Result<S extends Source> = SourceMap[S]['result'];
export interface Store {
/** The value kept under `name`; throws `InvalidAccessError` for a locked presence secret, read with `unlock` (LLP 1133.010). */
get(name: string): string | null;
/** Throws `NotSupportedError` when the name would be kept locked and this device keeps no locked secret. */
set(name: string, value: string): void;
forget(name: string): void;
/** A value is kept under `name`, locked or not. */
has(name: string): boolean;
/** `name` is kept under its presence policy, or awaits the move to it. */
locked(name: string): boolean;
/** The value: after the person's check when `name` is locked, at once when not; `null` when nothing is kept. Rejects with `code` `USER_CANCEL`, `AUTHENTICATION_FAILED`, `LOCKOUT`, `NOT_AVAILABLE`, `PERMISSION_DENIED` or `NotSupportedError`. */
unlock(name: string, options?: { reason?: string }): Promise<string | null>;
/** Lock an `optional` presence name for good; rejects `NotSupportedError` where no locked secret can be kept. */
protect(name: string): Promise<void>;
/** After the person's check, unlock an `optional` presence name for good. */
unprotect(name: string, options?: { reason?: string }): Promise<void>;
/** Keep a P-256 pair under the `secret.keep <name>` grant (LLP 1069.005 D1b). */
keepKey(name: string, pair: CryptoKeyPair): Promise<void>;
/** The pair kept under `name`, its private key non-extractable. */
key(name: string): Promise<CryptoKeyPair | null>;
}
declare global {
/** Sign in through the system browser (LLP 1069.006): the callback URL, or a rejection whose `status` is 499 cancelled, 403, 409, 428, 501 or 502. `message` is the sentence a loopback callback's page shows in place of "You can close this tab." (LLP 1133.005 D9). */
function openAuthSession(url: string, options: { callback: string; state: string; ephemeral?: boolean; message?: string }): Promise<string>;
/** This carrier's auth callback, known before PAR (LLP 1069.006 D2). */
function authCallback(): string;
}
export interface NativeModule { readonly available: boolean; call(request: Record<string, unknown>): Record<string, unknown>; later(request: Record<string, unknown>): Promise<Record<string, unknown>>; watch(topic: string): void }
/** A resource-owned WebSocket, for TypeScript on the web JS target and native hosts (LLP 1016.000 D7). Methods are synchronous-mutation-only; refresh or disposal invalidates it. */
export interface StreamSocket { readonly readyState: number; readonly bufferedAmount: number; send(data: string | ArrayBuffer | ArrayBufferView): void; close(code?: number, reason?: string): void }
/** A local listener's connection (LLP 1133.005 D1): `send` writes a line and a `\n` (a line with a `\n` throws `SyntaxError`); `end()` closes it once what was sent is written. `readyState` is 1 while open, 3 after its `end`. The same rules as `StreamSocket`'s, by connection: its listener's mapper sends on the connection it is handed. */
export interface ListenConnection { readonly readyState: number; send(line: string): void; end(): void }
/** A local listener's event: `listening` (`address`, `port`: 0 for `unix:`), then each `connection`, `message` (one line, without its terminator) and `end` (`reason` `peer`, `app`, `timeout` or `overflow`), by connection `id`; `error` ends it (`kind` `Refused`, `AddressInUse`, `Unsupported` or `Network`). */
export type ListenEvent = { readonly type: "listening"; readonly address: string; readonly port: number } | { readonly type: "connection"; readonly id: number } | { readonly type: "message"; readonly id: number; readonly data: string } | { readonly type: "end"; readonly id: number; readonly reason: "peer" | "app" | "timeout" | "overflow" } | { readonly type: "error"; readonly kind: string; readonly message: string };
declare global {
/** A local listener (LLP 1133.005), a resource's answer as `fetch(url, {exactStream})` is: listening while the resource holds it, on `unix:<absolute path>` under a `net.listen` grant. Defined on desktop hosts only: test `"exactListen" in globalThis`. A mutation that calls it throws `InvalidStateError`. */
function exactListen<T>(address: string, options: { exactStream: (event: ListenEvent, connection?: ListenConnection) => T; /** Newest wins: take the address over from a live listener. */ takeover?: boolean; /** ms, 1 to 600000: a connection whose first line has not arrived ends `timeout`. */ firstLineTimeout?: number; /** 1 to 1048576, 65536 by default. */ maxLineBytes?: number; /** 1 to 64, 16 by default. */ maxConnections?: number }): Promise<T>;
}
export interface StreamEvent { readonly type: string; readonly data: string | ArrayBuffer; readonly lastEventId: string; readonly coalesced: number; readonly kind?: string; readonly message?: string; readonly status?: number; readonly code?: number; readonly reason?: string; readonly wasClean?: boolean; /** An upload's `progress`: the body's bytes sent so far, of `total`. */ readonly loaded?: number; readonly total?: number; readonly lengthComputable?: boolean }
declare global {
interface RequestInit { exactStream?: (event: StreamEvent, socket?: StreamSocket) => unknown; exactIndependentHttp?: { maxResponseBytes: number }; /** A deadline for the whole exchange, 1 to 3600000 ms: the request is cancelled and the fetch rejects with a FetchError whose `kind` is "Timeout". Not with `exactStream`, but for an upload (`exactUploadProgress`). */ exactTimeout?: number; /** The body is this app file (`app:/…`), read by the host when it sends the request, under `fs.read`, at most 64 MiB; its bytes never enter JavaScript. Not with `body`, nor on a GET or HEAD; no Content-Type is added. */ exactBodyFrom?: string; /** A 2xx reply's body is written to this app file (`app:/…`) by the host as it arrives, under `fs.write`, at most `exactIndependentHttp.maxResponseBytes` or 1 GiB; its bytes never enter JavaScript. The file appears whole or not at all, and the response's `exactBodyTo` names the bytes written. A reply that is not 2xx leaves the file untouched and is read as usual. Not with `exactStream`, nor on a HEAD. */ exactBodyTo?: string; /** The server may hold the reply until something changes (a long poll): no clock waits on it (a jump, `clock settle`, `clock data`), and each reply that leaves it out names it. Its answer must be safe to supersede. Not with `exactStream`. */ exactHeld?: boolean; /** With `exactStream`, an upload: as its body (`body` or `exactBodyFrom`) goes out the mapper gets `progress` events (`loaded` of `total` bytes sent), then `load` with the reply's `status` and `data`, whatever the status, which ends it. It is in flight until that reply, for `pending` and every clock. Not on a GET or HEAD, a WebSocket or a long poll; it follows its redirects. */ exactUploadProgress?: boolean; /** With `exactStream`, a long poll: the host asks again and again, one GET at a time, each JSON reply an event whose `lastEventId` is its cursor (the reply's field `from`, `a` or `a.b`), asked with next in the query parameter `cursor`; a heartbeat (204, 304, an empty body, or by default the same cursor) is asked again with no event; the network, 408, 429, 502, 503 or 504 after `Retry-After` or a backoff of 1 s doubling to 30 s. */ exactLongPoll?: { cursor: string; from: string; sameCursor?: "heartbeat" | "event" } }
/** `{path, bytes}` when `exactBodyTo` wrote the body to its file (its `text()` and `arrayBuffer()` are then empty); absent or `null` otherwise. */
interface Response { readonly exactBodyTo?: { readonly path: string; readonly bytes: number } | null }
/** An answer that keeps coming: `exactStream` maps each event, and the end, to the answer. */
function fetch<T>(input: string | URL, init: RequestInit & { exactStream: (event: StreamEvent, socket?: StreamSocket) => T }): Promise<T>;
}
export type Sources = { [S in Source]: (args: Args<S>, store: Store, storage: Storage, native?: NativeModule | null) => Result<S> | Promise<Result<S>> };
export type Answer = <S extends Source>(source: S, args: Args<S>, store: Store, storage: Storage, native?: NativeModule | null) => Result<S> | Promise<Result<S>>;
/** A send shown over a resource it `refreshes`, in send order: `reply` once its reply has landed. `answered`: it landed and this answer was asked after it, so the answer should include it; call `keep()` to go on showing it when the answer does not include it yet. */
export type Write = { [S in Source]: { readonly id: number; readonly mutation: string; readonly source: S; readonly args: Args<S>; readonly reply?: Result<S>; readonly answered: boolean; keep(): void } }[Source];
/** What a resource shows while writes affect it: its answer with them laid over it. Synchronous, with no effects; `undefined` shows the answer and stops showing every answered write. */
export type Overlay = <S extends Source>(source: S, args: Args<S>, answer: Result<S>, writes: readonly Write[]) => Result<S> | undefined;
/** A colour token of the app's design, by its name (LLP 1121 D12). */
export type ColorToken = "AccentColor" | "brand.black" | "brand.white" | "brand.brown" | "brand.red" | "brand.orange" | "brand.amber" | "brand.yellow" | "brand.green" | "brand.default" | "brand.cyan" | "brand.blue" | "brand.purple" | "brand.violet" | "brand.pink" | "brand.magenta" | "brand.gray" | "ink.light" | "ink.dark" | "surface.bubble" | "surface.canvas" | "surface.card" | "surface.line" | "surface.raised" | "surface.sidebar" | "surface.soft" | "ink.muted" | "ink.faint";
/** A length token of the app's design, by its name (LLP 1121 D12). */
export type LengthToken = "space.xs" | "space.sm" | "space.md" | "space.lg" | "space.xl" | "radius.sm" | "radius.md" | "radius.lg" | "radius.full";
