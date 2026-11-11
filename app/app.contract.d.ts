// @generated from Contract source signatures; do not edit.
// Static types do not replace runtime shape, identity, or grant checks.
//
// What app.ts writes, in brief (the declarations follow): `sources: Sources` has one
// function per source, given its arguments as an array (then store, storage, native) and
// answering its shape, or a Promise of it.
//   closeComputer: (args: [string]) => Reply
//   computerControl: (args: [string, string, string]) => Reply
//   computerInput: (args: [string, string, number, number, number, number, number, string, number, number]) => DriveReply
//   createBot: (args: [string, string, string, string]) => Made
//   createGroup: (args: [string, string, string, string, string]) => Made
//   createSubagent: (args: [string, string, string, string, string, boolean]) => Reply
//   deleteBot: (args: [string]) => Reply
//   deleteGroup: (args: [string]) => Reply
//   deleteLogin: (args: [string]) => Reply
//   dropVoice: (args: [string, string]) => Reply
//   enableNotifications: (args: []) => Reply
//   hideBot: (args: [string, boolean]) => Reply
//   loadAbout: (args: []) => About
//   loadBots: (args: [string]) => Bot[]
//   loadChat: (args: [string, string]) => Chat
//   loadComputer: (args: [string, string, string, string, string]) => Computer
//   loadConvos: (args: [string, string, string, string, boolean]) => Convo[]
//   loadDrafts: (args: []) => DraftPack
//   loadLineage: (args: [string, string, string]) => Lineage
//   loadModels: (args: [string, string, string]) => ModelList
//   loadNetwork: (args: [string]) => Network
//   loadNotice: (args: []) => NoticeState
//   loadPrefs: (args: []) => Prefs
//   loadProviders: (args: []) => Provider[]
//   loadRoom: (args: [string]) => Room
//   loadRoutines: (args: [string, string, string]) => RoutineList
//   loadSession: (args: [string]) => Session
//   loadShelf: (args: [string, string]) => Shelf
//   loadSuggestions: (args: []) => Suggestion[]
//   loadSystem: (args: [string]) => SystemStatus
//   loadUsage: (args: [string, string]) => Usage
//   loadUsageReport: (args: [string, string, string, string]) => UsageReport
//   loadVault: (args: []) => Login[]
//   makeMainBot: (args: [string]) => Reply
//   markRoomSeen: (args: [string]) => Reply
//   markSeen: (args: [string]) => Reply
//   openComputer: (args: [string]) => Reply
//   pinBot: (args: [string, boolean]) => Reply
//   pinMessage: (args: [string, string, string, string]) => Reply
//   reactTo: (args: [string, string, string]) => Reply
//   readHealth: (args: []) => Health
//   recolourBot: (args: [string, string]) => Reply
//   renameBot: (args: [string, string]) => Reply
//   reshapeBot: (args: [string, string]) => Reply
//   respondRequest: (args: [string, string, string, string, string, string]) => Reply
//   restartGateway: (args: []) => Reply
//   revealPassword: (args: [string]) => Revealed
//   saveDrafts: (args: [string[], string[], string[]]) => Reply
//   saveLogin: (args: [string, string, string, string, string, string]) => Reply
//   saveSubagent: (args: [string, string, string, string, string, boolean]) => Reply
//   sendMessage: (args: [string, string, string, string, string, string]) => Reply
//   sendRoom: (args: [string, string]) => Reply
//   sendVoice: (args: [string, number, string, string]) => Reply
//   setBotNotifications: (args: [string, boolean]) => Reply
//   setDefaultMode: (args: [string]) => Reply
//   setDefaultModel: (args: [string, string, string]) => Reply
//   setModel: (args: [string, string, string, string, string]) => Reply
//   setNetwork: (args: [string]) => Reply
//   setNotifications: (args: [string, boolean, string, string]) => Reply
//   setPassword: (args: [string]) => Reply
//   setTheme: (args: [string]) => Reply
//   setUserName: (args: [string]) => Reply
//   signIn: (args: [string]) => Reply
//   signOut: (args: []) => Reply
//   stageVoice: (args: [string, string, string, number]) => VoiceHeld
//   startUpdate: (args: []) => UpdateStart
//   stopRun: (args: [string, string]) => Reply
//   updateGroup: (args: [string, string, string]) => Reply
//   uploadAttachment: (args: [string, string, string, string, string]) => Attached
// The shapes. None is exported: name one by its source, `type Reply = Result<'closeComputer'>`.
//   Reply = { ok: boolean; error: string }
//   DriveReply = { ok: boolean; error: string; text: string }
//   Session = { authenticated: boolean; passwordRequired: boolean }
//   Network = { mode: string; locked: boolean; restartRequired: boolean; canRestart: boolean; listening: string; urls: string; error: string; http: boolean }
//   Bot = { slug: string; name: string; label: string; line: string; time: string; initials: string; color: string; shape: string; main: boolean; pinned: boolean; hidden: boolean; ready: boolean; working: boolean; unread: boolean; parentSlug: string; hideFromSidebar: boolean; subagent: boolean; prompt: string; inheritParentPrompt: boolean; notifications: boolean }
//   FileRef = { id: string; name: string; kind: string; size: string; url: string }
//   Choice = { label: string; value: string; key: string; warn: string }
//   Part = { kind: string; text: string; head: string[]; rows: string[][]; align: string[] }
//   Row = { id: string; kind: string; mine: boolean; text: string; title: string; time: string; head: boolean; tail: boolean; variant: string; files: FileRef[]; botSlug: string; botName: string; botColor: string; botShape: string; choices: Choice[]; placeholder: string; key: string; react: string; quote: string; tokens: string; parts: Part[] }
//   Suggestion = { name: string; about: string; color: string; shape: string; first: string }
//   About = { ok: boolean; version: string; harness: string; connected: boolean }
//   SystemStatus = { ok: boolean; installed: boolean; service: boolean; version: string; latest: string; updateAvailable: boolean; error: string }
//   UpdateStart = { ok: boolean; version: string; error: string }
//   Health = { ok: boolean; version: string }
//   Pin = { id: string; text: string; missing: boolean }
//   Chat = { slug: string; rows: Row[]; working: boolean; partial: boolean; readOnly: boolean; pins: Pin[]; threadId: string }
//   RoomMember = { slug: string; name: string; line: string; state: string; ready: boolean; working: boolean }
//   RoomMessage = { id: string; name: string; text: string; from: string; parts: Part[] }
//   Room = { id: string; title: string; description: string; color: string; shape: string; members: RoomMember[]; messages: RoomMessage[]; notifications: boolean }
//   FaceBit = { shape: string; color: string; state: string }
//   Convo = { kind: string; key: string; id: string; name: string; color: string; shape: string; faces: FaceBit[]; line: string; time: string; at: string; unread: boolean; working: boolean; ready: boolean; notifications: boolean }
//   Made = { ok: boolean; id: string; error: string }
//   Prefs = { userName: string; theme: string; defaultInstance: string; defaultModel: string; defaultEffort: string; defaultRuntimeMode: string; noticeEnabled: boolean; noticeSound: boolean; noticePreview: string; noticeWhen: string; noticeEnabledMissing: boolean }
//   NoticeState = { available: boolean; state: string }
//   Attached = { ok: boolean; id: string; name: string; error: string }
//   VoiceHeld = { ok: boolean; id: string; name: string; url: string; seconds: number; error: string }
//   Login = { id: string; title: string; website: string; username: string; hasPassword: boolean; hints: string; icon: string }
//   Revealed = { ok: boolean; id: string; password: string; error: string }
//   Usage = { ok: boolean; error: string; messages: number; attachments: number; vaultWrites: number; iconFetches: number }
//   Effort = { id: string; label: string }
//   ModelOption = { slug: string; name: string; recommended: boolean; effort: string; efforts: Effort[] }
//   Provider = { instanceId: string; name: string; account: string; models: ModelOption[] }
//   ModelList = { ok: boolean; error: string; current: string; instanceId: string; effort: string; runtimeMode: string; currentName: string; providerName: string; effortLabel: string; efforts: Effort[]; providers: Provider[] }
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
type T3 = { "ok": T0; "error": T1; "text": T1; };
type T4 = { "authenticated": T0; "passwordRequired": T0; };
type T5 = { "mode": T1; "locked": T0; "restartRequired": T0; "canRestart": T0; "listening": T1; "urls": T1; "error": T1; "http": T0; };
type T6 = { "slug": T1; "name": T1; "label": T1; "line": T1; "time": T1; "initials": T1; "color": T1; "shape": T1; "main": T0; "pinned": T0; "hidden": T0; "ready": T0; "working": T0; "unread": T0; "parentSlug": T1; "hideFromSidebar": T0; "subagent": T0; "prompt": T1; "inheritParentPrompt": T0; "notifications": T0; };
type T7 = { "id": T1; "name": T1; "kind": T1; "size": T1; "url": T1; };
type T8 = { "label": T1; "value": T1; "key": T1; "warn": T1; };
type T9 = T7[];
type T10 = T8[];
type T11 = { "id": T1; "kind": T1; "mine": T0; "text": T1; "title": T1; "time": T1; "head": T0; "tail": T0; "variant": T1; "files": T9; "botSlug": T1; "botName": T1; "botColor": T1; "botShape": T1; "choices": T10; "placeholder": T1; "key": T1; "react": T1; "quote": T1; "tokens": T1; "parts": { "kind": T1; "text": T1; "head": T1[]; "rows": T1[][]; "align": T1[] }[]; };
type T12 = { "name": T1; "about": T1; "color": T1; "shape": T1; "first": T1; };
type T13 = { "ok": T0; "version": T1; "harness": T1; "connected": T0; };
type T14 = { "ok": T0; "installed": T0; "service": T0; "version": T1; "latest": T1; "updateAvailable": T0; "error": T1; };
type T15 = { "ok": T0; "version": T1; "error": T1; };
type T16 = { "ok": T0; "version": T1; };
type T17 = { "id": T1; "text": T1; "missing": T0; };
type T18 = T11[];
type T19 = T17[];
type T20 = { "slug": T1; "rows": T18; "working": T0; "partial": T0; "readOnly": T0; "pins": T19; "threadId": T1; };
type T21 = { "slug": T1; "name": T1; "line": T1; "state": T1; "ready": T0; "working": T0; };
type T22 = { "id": T1; "name": T1; "text": T1; "from": T1; "parts": { "kind": T1; "text": T1; "head": T1[]; "rows": T1[][]; "align": T1[] }[]; };
type T23 = T21[];
type T24 = T22[];
type T25 = { "id": T1; "title": T1; "description": T1; "color": T1; "shape": T1; "members": T23; "messages": T24; "notifications": T0; };
type T26 = { "slug": T1; "name": T1; "color": T1; "shape": T1; };
type T27 = { "shape": T1; "color": T1; "state": T1; };
type T28 = T27[];
type T29 = { "kind": T1; "key": T1; "id": T1; "name": T1; "color": T1; "shape": T1; "faces": T28; "line": T1; "time": T1; "at": T1; "unread": T0; "working": T0; "ready": T0; "notifications": T0; };
type T30 = { "ok": T0; "id": T1; "error": T1; };
type T31 = { "userName": T1; "theme": T1; "defaultInstance": T1; "defaultModel": T1; "defaultEffort": T1; "defaultRuntimeMode": T1; "noticeEnabled": T0; "noticeSound": T0; "noticePreview": T1; "noticeWhen": T1; "noticeEnabledMissing": T0; };
type T32 = { "visibilityState": T1; "hasFocus": T0; };
type T33 = { "available": T0; "state": T1; };
type T34 = { "ok": T0; "id": T1; "name": T1; "error": T1; };
type T35 = number;
type T36 = { "ok": T0; "id": T1; "name": T1; "url": T1; "seconds": T35; "error": T1; };
type T37 = { "id": T1; "title": T1; "website": T1; "username": T1; "hasPassword": T0; "hints": T1; "icon": T1; };
type T38 = { "ok": T0; "id": T1; "password": T1; "error": T1; };
type T39 = { "ok": T0; "error": T1; "messages": T35; "attachments": T35; "vaultWrites": T35; "iconFetches": T35; };
type T40 = { "id": T1; "label": T1; };
type T41 = T40[];
type T42 = { "slug": T1; "name": T1; "recommended": T0; "effort": T1; "efforts": T41; };
type T43 = T42[];
type T44 = { "instanceId": T1; "name": T1; "account": T1; "models": T43; };
type T45 = T44[];
type T46 = { "ok": T0; "error": T1; "current": T1; "instanceId": T1; "effort": T1; "runtimeMode": T1; "currentName": T1; "providerName": T1; "effortLabel": T1; "efforts": T41; "providers": T45; };
type T47 = { "id": T1; "title": T1; "paused": T0; "schedule": T1; "next": T1; "failed": T0; };
type T48 = T47[];
type T49 = { "ok": T0; "tasks": T48; "error": T1; };
type T50 = { "id": T1; "title": T1; "status": T1; "remote": T0; "threadId": T1; };
type T51 = T50[];
type T52 = { "ok": T0; "rows": T51; "error": T1; };
type T53 = { "day": T1; "label": T1; "short": T1; "tokens": T1; "input": T1; "output": T1; "cached": T1; "cacheWrite": T1; "turns": T1; "messages": T1; "px": T35; "spx": T35; };
type T54 = { "name": T1; "tokens": T1; "turns": T1; "share": T35; };
type T55 = { "slug": T1; "name": T1; "color": T1; "shape": T1; "tokens": T1; "turns": T1; "messages": T1; "share": T35; };
type T56 = T53[];
type T57 = T54[];
type T58 = T55[];
type T59 = { "ok": T0; "error": T1; "period": T1; "periodLabel": T1; "messages": T1; "attachments": T1; "turns": T1; "tokens": T1; "input": T1; "output": T1; "cached": T1; "cacheWrite": T1; "hit": T1; "sentence": T1; "days": T56; "rows": T56; "models": T57; "bots": T58; "empty": T0; "first": T1; "last": T1; };
type T60 = { "id": T1; "title": T1; "detail": T1; "url": T1; };
type T61 = T60[];
type T62 = { "ok": T0; "total": T35; "pictures": T61; "files": T61; "links": T61; };
type T63 = { "id": T1; "title": T1; "url": T1; };
type T64 = T63[];
type T65 = { "ok": T0; "tabs": T64; "image": T1; "error": T1; "selected": T1; };
type T66 = T1[];
type T67 = { "stamp": T1; "texts": T66; "files": T66; "voices": T66; };
type T68 = { "locale": T1; };
type T69 = { "hover": T1; };
type T70 = { "accessibilityTextSize": T0; };
type T71 = { "prefersContrast": T1; };
type T72 = { "permanentlyDenied": T0; "shouldShowRationale": T0; };
type T73 = T72 | null;
type T74 = "notDetermined" | "restricted" | "denied" | "authorized";
type T75 = { "authorizationStatus": T74; };
type T76 = T75 | null;
type T77 = "granted" | "prompt" | "denied" | "unknown";
type T78 = { "android": T73; "apple": T76; "available": T0; "state": T77; };
type T79 = T35 | null;
type T80 = { "max": T35; "min": T35; "step": T79; };
type T81 = T1 | null;
type T82 = { "refused": T81; };
type T83 = "audioinput" | "videoinput" | "audiooutput";
type T84 = { "deviceId": T1; "facingMode": T81; "groupId": T1; "kind": T83; "label": T1; };
type T85 = T84[];
type T86 = { "devices": T85; };
type T87 = { "cameraId": T81; "isClientSilenced": T0; "lensFacing": T81; };
type T88 = T87 | null;
type T89 = { "deviceType": T81; "portName": T81; "portType": T81; "position": T81; };
type T90 = T89 | null;
type T91 = T80 | null;
type T92 = T66 | null;
type T93 = T0 | null;
type T94 = { "aspectRatio": T91; "facingMode": T92; "frameRate": T91; "height": T91; "torch": T93; "width": T91; "zoom": T91; };
type T95 = T94 | null;
type T96 = "audio" | "video";
type T97 = "idle" | "interrupted" | "background" | "silenced" | "in-use" | "multitasking" | "system-pressure";
type T98 = T97 | null;
type T99 = "live" | "ended";
type T100 = { "aspectRatio": T79; "autoGainControl": T93; "channelCount": T79; "deviceId": T81; "echoCancellation": T93; "facingMode": T81; "frameRate": T79; "groupId": T81; "height": T79; "latency": T79; "noiseSuppression": T93; "resizeMode": T81; "sampleRate": T79; "sampleSize": T79; "torch": T93; "width": T79; "zoom": T79; };
type T101 = { "android": T88; "apple": T90; "capabilities": T95; "enabled": T0; "id": T1; "kind": T96; "label": T1; "muteReason": T98; "muted": T0; "readyState": T99; "settings": T100; };
type T102 = T101[];
type T103 = { "active": T0; "id": T1; "tracks": T102; };
type T104 = { "aspectRatio": T79; "autoGainControl": T93; "channelCount": T79; "deviceId": T81; "echoCancellation": T93; "facingMode": T81; "frameRate": T79; "groupId": T81; "height": T79; "latency": T79; "noiseSuppression": T93; "resizeMode": T81; "sampleRate": T79; "sampleSize": T79; "torch": T93; "width": T79; "zoom": T79; };
type T105 = { "permanentlyDenied": T0; "shouldShowRationale": T0; };
type T106 = T105 | null;
type T107 = "undetermined" | "denied" | "granted" | "notDetermined" | "restricted" | "authorized";
type T108 = { "recordPermission": T107; };
type T109 = T108 | null;
type T110 = "granted" | "prompt" | "denied" | "unknown";
type T111 = { "android": T106; "apple": T109; "available": T0; "state": T110; };
type T112 = { "x": T35; "y": T35; };
type T113 = T35[];
type T114 = { "audioLevel": T35; "levels": T113; "totalAudioEnergy": T35; "totalSamplesDuration": T35; };
type T115 = "requested" | "interrupted" | "media-reset" | "background" | "track-ended" | "error" | "session-ended";
type T116 = T115 | null;
type T117 = { "duration": T35; "mimeType": T1; "path": T1; "size": T35; };
type T118 = T117 | null;
type T119 = "inactive" | "recording" | "paused";
type T120 = { "audioBitsPerSecond": T79; "duration": T35; "error": T81; "id": T1; "mimeType": T1; "path": T1; "reason": T116; "result": T118; "state": T119; };
type T121 = { "supported": T0; "type": T1; };
type T122 = { "body": T81; "data": T81; "tag": T1; "title": T1; };
type T123 = "notSupported" | "disabled" | "enabled";
type T124 = "notDetermined" | "denied" | "authorized" | "provisional" | "ephemeral";
type T125 = "notSupported" | "disabled" | "enabled";
type T126 = "notSupported" | "disabled" | "enabled";
type T127 = { "alertSetting": T123; "authorizationStatus": T124; "badgeSetting": T125; "soundSetting": T126; };
type T128 = T127 | null;
type T129 = "granted" | "prompt" | "denied" | "unknown";
type T130 = { "apple": T128; "available": T0; "badge": T0; "state": T129; };
type T131 = "mic" | "camcorder" | "voiceRecognition" | "voiceCommunication" | "unprocessed" | "voicePerformance";
type T132 = T131 | null;
type T133 = { "audioSource": T132; "privacySensitive": T93; };
type T134 = "mixWithOthers" | "duckOthers" | "allowBluetoothHFP" | "allowBluetoothA2DP" | "defaultToSpeaker" | "bluetoothHighQualityRecording" | "farFieldInput" | "overrideMutedMicrophoneInterruption";
type T135 = T134[];
type T136 = T135 | null;
type T137 = "builtInWideAngleCamera" | "builtInUltraWideCamera" | "builtInTelephotoCamera" | "builtInDualCamera" | "builtInDualWideCamera" | "builtInTripleCamera" | "builtInTrueDepthCamera" | "builtInLiDARDepthCamera" | "continuityCamera" | "deskViewCamera" | "external";
type T138 = T137 | null;
type T139 = "default" | "measurement" | "voiceChat" | "videoChat" | "videoRecording";
type T140 = T139 | null;
type T141 = "stereo" | "cardioid" | "omnidirectional";
type T142 = T141 | null;
type T143 = { "categoryOptions": T136; "deviceType": T138; "inputDataSource": T81; "mode": T140; "polarPattern": T142; "preferredIOBufferDuration": T79; "prefersNoInterruptionsFromSystemAlerts": T93; };
type T144 = T104 | null;
type T145 = T133 | null;
type T146 = T143 | null;
type T147 = { "audio": T93; "audioConstraints": T144; "video": T93; "videoConstraints": T144; "android": T145; "apple": T146; };
type T148 = "builtInWideAngleCamera" | "builtInUltraWideCamera" | "builtInTelephotoCamera" | "builtInDualCamera" | "builtInDualWideCamera" | "builtInTripleCamera" | "builtInTrueDepthCamera" | "builtInLiDARDepthCamera" | "continuityCamera" | "deskViewCamera" | "external";
type T149 = T148 | null;
type T150 = "locked" | "autoExpose" | "continuousAutoExposure";
type T151 = T150 | null;
type T152 = T112 | null;
type T153 = "locked" | "autoFocus" | "continuousAutoFocus";
type T154 = T153 | null;
type T155 = { "deviceType": T149; "exposureMode": T151; "exposurePointOfInterest": T152; "focusMode": T154; "focusPointOfInterest": T152; };
type T156 = T155 | null;
type T157 = { "aspectRatio": T79; "facingMode": T81; "frameRate": T79; "height": T79; "resizeMode": T81; "torch": T93; "track": T1; "width": T79; "zoom": T79; "apple": T156; };
type T158 = { "track": T1; };
type T159 = { "enabled": T0; "track": T1; };
type T160 = { "track": T1; };
type T161 = "aac" | "heAac" | "opus";
type T162 = T161 | null;
type T163 = "mpeg4" | "ogg" | "webm";
type T164 = T163 | null;
type T165 = { "audioEncoder": T162; "outputFormat": T164; };
type T166 = "min" | "low" | "medium" | "high" | "max";
type T167 = T166 | null;
type T168 = "constant" | "longTermAverage" | "variableConstrained" | "variable";
type T169 = T168 | null;
type T170 = "aac" | "alac" | "linearPCM" | "opus";
type T171 = T170 | null;
type T172 = { "audioQuality": T167; "bitRateStrategy": T169; "formatID": T171; };
type T173 = "constant" | "variable";
type T174 = T173 | null;
type T175 = T165 | null;
type T176 = T172 | null;
type T177 = { "audioBitrateMode": T174; "audioBitsPerSecond": T79; "mimeType": T81; "path": T1; "stream": T1; "android": T175; "apple": T176; };
type T178 = { "recorder": T1; };
type T179 = { "recorder": T1; };
type T180 = { "recorder": T1; };
type T181 = { "interval": T79; "recorder": T1; };
type T182 = { "type": T1; };
type T183 = { "history": T79; "interval": T79; "track": T1; };
type T184 = "passive" | "active" | "timeSensitive";
type T185 = T184 | null;
type T186 = { "interruptionLevel": T185; "relevanceScore": T79; "subtitle": T81; "targetContentIdentifier": T81; "threadIdentifier": T81; };
type T187 = T186 | null;
type T188 = { "body": T81; "data": T81; "navigate": T81; "showTrigger": T79; "silent": T93; "tag": T81; "title": T1; "apple": T187; };
type T189 = { "tag": T1; };
type T190 = { "contents": T79; };
type T191 = { "slug": T1; "id": T1; };
type T192 = { "id": T35; "name": T1; "url": T1; "tab": T1; "params": T191; "fragment": T1; };
type T193 = T192[];
type T194 = { "name": T1; "stack": T193; };
type T195 = T194[];
type T196 = { "tab": T1; "tabs": T195; "next": T35; };
type T197 = T26[];
type T198 = T6[];
type T199 = T37[];
type T200 = T29[];
type T201 = T12[];
type T202 = null;
type T203 = T2 | null;
type T204 = T30 | null;
type T205 = T130 | null;
type T206 = T15 | null;
type T207 = T16 | null;
type T208 = T38 | null;
type T209 = T34 | null;
type T210 = T103 | null;
type T211 = T120 | null;
type T212 = T101 | null;
type T213 = T36 | null;
type T214 = T3 | null;
type T215 = { "key": T1; "shiftKey": T0; "ctrlKey": T0; "altKey": T0; "metaKey": T0; "code": T1; "repeat": T0; "isComposing": T0; };
type T216 = { "path": T1; "name": T1; "type": T1; "size": T35; "width": T79; "height": T79; "duration": T79; };
type T217 = T216[];
type T218 = { "offsetX": T35; "offsetY": T35; "files": T66; "shiftKey": T0; "ctrlKey": T0; "altKey": T0; "metaKey": T0; };
type T219 = { "scrollLeft": T35; "scrollTop": T35; "scrollWidth": T35; "scrollHeight": T35; "clientWidth": T35; "clientHeight": T35; "firstVisibleKey": T81; "firstVisibleOffset": T35; };
type T220 = { "text": T1; "files": T66; };
type T221 = { "offsetX": T35; "offsetY": T35; "button": T35; "buttons": T35; "pressure": T35; "pointerType": T1; "pointerId": T35; "clientX": T35; "clientY": T35; "shiftKey": T0; "ctrlKey": T0; "altKey": T0; "metaKey": T0; };
type T222 = { "offsetX": T35; "offsetY": T35; "deltaX": T35; "deltaY": T35; "deltaMode": T35; "shiftKey": T0; "ctrlKey": T0; "altKey": T0; "metaKey": T0; };
type T223 = { "target": T1; "relatedTarget": T81; };

/** Sources called by compiled mutation sends. */
export type MutationSource = "closeComputer" | "computerControl" | "computerInput" | "createBot" | "createGroup" | "createSubagent" | "deleteBot" | "deleteGroup" | "deleteLogin" | "dropVoice" | "enableNotifications" | "hideBot" | "makeMainBot" | "markRoomSeen" | "markSeen" | "openComputer" | "pinBot" | "pinMessage" | "reactTo" | "readHealth" | "recolourBot" | "renameBot" | "reshapeBot" | "respondRequest" | "restartGateway" | "revealPassword" | "saveDrafts" | "saveLogin" | "saveSubagent" | "sendMessage" | "sendRoom" | "sendVoice" | "setBotNotifications" | "setDefaultMode" | "setDefaultModel" | "setModel" | "setNetwork" | "setNotifications" | "setPassword" | "setTheme" | "setUserName" | "signIn" | "signOut" | "stageVoice" | "startUpdate" | "stopRun" | "updateGroup" | "uploadAttachment";

export interface SourceMap {
  "closeComputer": { args: [T1]; result: T2 };
  "computerControl": { args: [T1, T1, T1]; result: T2 };
  "computerInput": { args: [T1, T1, T35, T35, T35, T35, T35, T1, T35, T35]; result: T3 };
  "createBot": { args: [T1, T1, T1, T1]; result: T30 };
  "createGroup": { args: [T1, T1, T1, T1, T1]; result: T30 };
  "createSubagent": { args: [T1, T1, T1, T1, T1, T0]; result: T2 };
  "deleteBot": { args: [T1]; result: T2 };
  "deleteGroup": { args: [T1]; result: T2 };
  "deleteLogin": { args: [T1]; result: T2 };
  "dropVoice": { args: [T1, T1]; result: T2 };
  "enableNotifications": { args: []; result: T2 };
  "hideBot": { args: [T1, T0]; result: T2 };
  "loadAbout": { args: []; result: T13 };
  "loadBots": { args: [T1]; result: T198 };
  "loadChat": { args: [T1, T1]; result: T20 };
  "loadComputer": { args: [T1, T1, T1, T1, T1]; result: T65 };
  "loadConvos": { args: [T1, T1, T1, T1, T0]; result: T200 };
  "loadDrafts": { args: []; result: T67 };
  "loadLineage": { args: [T1, T1, T1]; result: T52 };
  "loadModels": { args: [T1, T1, T1]; result: T46 };
  "loadNetwork": { args: [T1]; result: T5 };
  "loadNotice": { args: []; result: T33 };
  "loadPrefs": { args: []; result: T31 };
  "loadProviders": { args: []; result: T45 };
  "loadRoom": { args: [T1]; result: T25 };
  "loadRoutines": { args: [T1, T1, T1]; result: T49 };
  "loadSession": { args: [T1]; result: T4 };
  "loadShelf": { args: [T1, T1]; result: T62 };
  "loadSuggestions": { args: []; result: T201 };
  "loadSystem": { args: [T1]; result: T14 };
  "loadUsage": { args: [T1, T1]; result: T39 };
  "loadUsageReport": { args: [T1, T1, T1, T1]; result: T59 };
  "loadVault": { args: []; result: T199 };
  "makeMainBot": { args: [T1]; result: T2 };
  "markRoomSeen": { args: [T1]; result: T2 };
  "markSeen": { args: [T1]; result: T2 };
  "openComputer": { args: [T1]; result: T2 };
  "pinBot": { args: [T1, T0]; result: T2 };
  "pinMessage": { args: [T1, T1, T1, T1]; result: T2 };
  "reactTo": { args: [T1, T1, T1]; result: T2 };
  "readHealth": { args: []; result: T16 };
  "recolourBot": { args: [T1, T1]; result: T2 };
  "renameBot": { args: [T1, T1]; result: T2 };
  "reshapeBot": { args: [T1, T1]; result: T2 };
  "respondRequest": { args: [T1, T1, T1, T1, T1, T1]; result: T2 };
  "restartGateway": { args: []; result: T2 };
  "revealPassword": { args: [T1]; result: T38 };
  "saveDrafts": { args: [T66, T66, T66]; result: T2 };
  "saveLogin": { args: [T1, T1, T1, T1, T1, T1]; result: T2 };
  "saveSubagent": { args: [T1, T1, T1, T1, T1, T0]; result: T2 };
  "sendMessage": { args: [T1, T1, T1, T1, T1, T1]; result: T2 };
  "sendRoom": { args: [T1, T1]; result: T2 };
  "sendVoice": { args: [T1, T35, T1, T1]; result: T2 };
  "setBotNotifications": { args: [T1, T0]; result: T2 };
  "setDefaultMode": { args: [T1]; result: T2 };
  "setDefaultModel": { args: [T1, T1, T1]; result: T2 };
  "setModel": { args: [T1, T1, T1, T1, T1]; result: T2 };
  "setNetwork": { args: [T1]; result: T2 };
  "setNotifications": { args: [T1, T0, T1, T1]; result: T2 };
  "setPassword": { args: [T1]; result: T2 };
  "setTheme": { args: [T1]; result: T2 };
  "setUserName": { args: [T1]; result: T2 };
  "signIn": { args: [T1]; result: T2 };
  "signOut": { args: []; result: T2 };
  "stageVoice": { args: [T1, T1, T1, T35]; result: T36 };
  "startUpdate": { args: []; result: T15 };
  "stopRun": { args: [T1, T1]; result: T2 };
  "updateGroup": { args: [T1, T1, T1]; result: T2 };
  "uploadAttachment": { args: [T1, T1, T1, T1, T1]; result: T34 };
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
