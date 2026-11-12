// @generated from Contract source signatures; do not edit.
// Static types do not replace runtime shape, identity, or grant checks.
//
// What app.ts writes, in brief (the declarations follow): `sources: Sources` has one
// function per source, given its arguments as an array (then store, storage, native) and
// answering its shape, or a Promise of it.
//   checkAnytype: (args: []) => AnytypeCheck
//   closeComputer: (args: [string]) => Reply
//   computerControl: (args: [string, string, string]) => Reply
//   computerInput: (args: [string, string, number, number, number, number, number, string, number, number]) => DriveReply
//   confirmPairing: (args: [string, string]) => Reply
//   copyStorage: (args: [string]) => Reply
//   createBot: (args: [string, string, string, string, string]) => Made
//   createGroup: (args: [string, string, string, string, string, string, string]) => Made
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
//   loadConvos: (args: [string, string, string]) => Convo[]
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
//   loadStorage: (args: [string]) => Storage
//   loadSuggestions: (args: []) => Suggestion[]
//   loadSystem: (args: [string]) => SystemStatus
//   loadUsage: (args: [string, string]) => Usage
//   loadUsageReport: (args: [string, string, string, string]) => UsageReport
//   loadVault: (args: []) => Login[]
//   makeMainBot: (args: [string]) => Reply
//   markRoomSeen: (args: [string]) => Reply
//   markSeen: (args: [string]) => Reply
//   noteUnread: (args: [string, boolean, boolean, string, boolean, boolean, boolean, string, string, string, string]) => Reply
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
//   restartStorage: (args: []) => Reply
//   revealPassword: (args: [string]) => Revealed
//   saveBotPrompt: (args: [string, string]) => Reply
//   saveDrafts: (args: [string[], string[], string[]]) => Reply
//   saveLogin: (args: [string, string, string, string, string, string]) => Reply
//   saveSpace: (args: [string]) => Reply
//   saveStorage: (args: [string, string, string]) => Reply
//   saveSubagent: (args: [string, string, string, string, string, boolean]) => Reply
//   sendMessage: (args: [string, string, string, string, string, string, string]) => Sent
//   sendRoom: (args: [string, string, string]) => Sent
//   sendVoice: (args: [string, number, string, string]) => Reply
//   setBotNotifications: (args: [string, boolean]) => Reply
//   setDefaultMode: (args: [string]) => Reply
//   setDefaultModel: (args: [string, string, string]) => Reply
//   setGroupAnswerPolicy: (args: [string]) => Reply
//   setModel: (args: [string, string, string, string, string]) => Reply
//   setNetwork: (args: [string]) => Reply
//   setNotifications: (args: [string, boolean, string, string]) => Reply
//   setPassword: (args: [string]) => Reply
//   setTheme: (args: [string]) => Reply
//   setUserName: (args: [string]) => Reply
//   signIn: (args: [string]) => Reply
//   signOut: (args: []) => Reply
//   stageVoice: (args: [string, string, string, number]) => VoiceHeld
//   startPairing: (args: []) => Pairing
//   startUpdate: (args: []) => UpdateStart
//   stopRun: (args: [string, string]) => Reply
//   testStorage: (args: [string]) => Reply
//   updateGroup: (args: [string, string, string]) => Reply
//   uploadAttachment: (args: [string, string, string, string, string]) => Attached
// The shapes. None is exported: name one by its source, `type AnytypeCheck = Result<'checkAnytype'>`.
//   Bot = { slug: string; name: string; label: string; line: string; time: string; initials: string; color: string; shape: string; main: boolean; pinned: boolean; hidden: boolean; ready: boolean; working: boolean; unread: boolean; parentSlug: string; hideFromSidebar: boolean; subagent: boolean; prompt: string; inheritParentPrompt: boolean; notifications: boolean }
//   FileRef = { id: string; name: string; kind: string; size: string; url: string }
//   Choice = { label: string; value: string; key: string; warn: string }
//   Part = { kind: string; text: string; head: string[]; rows: string[][]; align: string[] }
//   Row = { id: string; kind: string; mine: boolean; text: string; title: string; time: string; head: boolean; tail: boolean; variant: string; files: FileRef[]; botSlug: string; botName: string; botColor: string; botShape: string; choices: Choice[]; placeholder: string; key: string; react: string; quote: string; tokens: string; resetAt: string; parts: Part[] }
//   Pin = { id: string; text: string; missing: boolean }
//   Chat = { slug: string; rows: Row[]; working: boolean; partial: boolean; readOnly: boolean; pins: Pin[]; threadId: string }
//   RoomMember = { slug: string; name: string; line: string; state: string; ready: boolean; working: boolean }
//   RoomMessage = { id: string; name: string; text: string; from: string; parts: Part[] }
//   Room = { id: string; title: string; description: string; color: string; shape: string; members: RoomMember[]; messages: RoomMessage[]; notifications: boolean; answerPolicy: string; answerPolicyEffective: string; chief: string }
//   FaceBit = { shape: string; color: string; state: string }
//   Convo = { kind: string; key: string; id: string; name: string; color: string; shape: string; faces: FaceBit[]; line: string; time: string; at: string; unread: boolean; working: boolean; ready: boolean; notifications: boolean }
//   Login = { id: string; title: string; website: string; username: string; hasPassword: boolean; hints: string; icon: string }
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
//   Reply = { ok: boolean; error: string }
//   Sent = { ok: boolean; error: string; messageId: string }
//   DriveReply = { ok: boolean; error: string; text: string }
//   Session = { authenticated: boolean; passwordRequired: boolean }
//   Network = { mode: string; locked: boolean; restartRequired: boolean; canRestart: boolean; listening: string; urls: string; error: string; http: boolean }
//   Storage = { backend: string; root: string; backendLock: string; rootLock: string; restartRequired: boolean; canRestart: boolean; error: string; keySet: boolean; space: string; spaceLock: string }
//   Pairing = { ok: boolean; error: string; challengeId: string }
//   AnytypeCheck = { ok: boolean; error: string; space: string; spaceFound: boolean; shared: boolean; schemaReady: boolean }
//   Suggestion = { name: string; about: string; color: string; shape: string; first: string }
//   About = { ok: boolean; version: string; harness: string; connected: boolean }
//   SystemStatus = { ok: boolean; installed: boolean; service: boolean; version: string; latest: string; updateAvailable: boolean; error: string }
//   UpdateStart = { ok: boolean; version: string; error: string }
//   Health = { ok: boolean; version: string }
//   Made = { ok: boolean; id: string; error: string }
//   Prefs = { userName: string; theme: string; defaultInstance: string; defaultModel: string; defaultEffort: string; defaultRuntimeMode: string; noticeEnabled: boolean; noticeSound: boolean; noticePreview: string; noticeWhen: string; noticeEnabledMissing: boolean; groupAnswerPolicy: string }
//   NoticeState = { available: boolean; state: string }
//   Attached = { ok: boolean; id: string; name: string; error: string }
//   VoiceHeld = { ok: boolean; id: string; name: string; url: string; seconds: number; error: string }
//   Revealed = { ok: boolean; id: string; password: string; error: string }
//   Usage = { ok: boolean; error: string; messages: number; attachments: number; vaultWrites: number; iconFetches: number }
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

type T0 = string;
type T1 = boolean;
type T2 = { "slug": T0; "name": T0; "label": T0; "line": T0; "time": T0; "initials": T0; "color": T0; "shape": T0; "main": T1; "pinned": T1; "hidden": T1; "ready": T1; "working": T1; "unread": T1; "parentSlug": T0; "hideFromSidebar": T1; "subagent": T1; "prompt": T0; "inheritParentPrompt": T1; "notifications": T1; };
type T3 = { "id": T0; "name": T0; "kind": T0; "size": T0; "url": T0; };
type T4 = { "label": T0; "value": T0; "key": T0; "warn": T0; };
type T5 = T0[];
type T6 = T5[];
type T7 = { "kind": T0; "text": T0; "head": T5; "rows": T6; "align": T5; };
type T8 = T3[];
type T9 = T4[];
type T10 = T7[];
type T11 = { "id": T0; "kind": T0; "mine": T1; "text": T0; "title": T0; "time": T0; "head": T1; "tail": T1; "variant": T0; "files": T8; "botSlug": T0; "botName": T0; "botColor": T0; "botShape": T0; "choices": T9; "placeholder": T0; "key": T0; "react": T0; "quote": T0; "tokens": T0; "resetAt": T0; "parts": T10; };
type T12 = { "id": T0; "text": T0; "missing": T1; };
type T13 = T11[];
type T14 = T12[];
type T15 = { "slug": T0; "rows": T13; "working": T1; "partial": T1; "readOnly": T1; "pins": T14; "threadId": T0; };
type T16 = { "slug": T0; "name": T0; "line": T0; "state": T0; "ready": T1; "working": T1; };
type T17 = { "id": T0; "name": T0; "text": T0; "from": T0; "parts": T10; };
type T18 = T16[];
type T19 = T17[];
type T20 = { "id": T0; "title": T0; "description": T0; "color": T0; "shape": T0; "members": T18; "messages": T19; "notifications": T1; "answerPolicy": T0; "answerPolicyEffective": T0; "chief": T0; };
type T21 = { "slug": T0; "name": T0; "color": T0; "shape": T0; };
type T22 = { "shape": T0; "color": T0; "state": T0; };
type T23 = T22[];
type T24 = { "kind": T0; "key": T0; "id": T0; "name": T0; "color": T0; "shape": T0; "faces": T23; "line": T0; "time": T0; "at": T0; "unread": T1; "working": T1; "ready": T1; "notifications": T1; };
type T25 = { "id": T0; "title": T0; "website": T0; "username": T0; "hasPassword": T1; "hints": T0; "icon": T0; };
type T26 = { "id": T0; "label": T0; };
type T27 = T26[];
type T28 = { "slug": T0; "name": T0; "recommended": T1; "effort": T0; "efforts": T27; };
type T29 = T28[];
type T30 = { "instanceId": T0; "name": T0; "account": T0; "models": T29; };
type T31 = T30[];
type T32 = { "ok": T1; "error": T0; "current": T0; "instanceId": T0; "effort": T0; "runtimeMode": T0; "currentName": T0; "providerName": T0; "effortLabel": T0; "efforts": T27; "providers": T31; };
type T33 = { "id": T0; "title": T0; "paused": T1; "schedule": T0; "next": T0; "failed": T1; };
type T34 = T33[];
type T35 = { "ok": T1; "tasks": T34; "error": T0; };
type T36 = { "id": T0; "title": T0; "status": T0; "remote": T1; "threadId": T0; };
type T37 = T36[];
type T38 = { "ok": T1; "rows": T37; "error": T0; };
type T39 = number;
type T40 = { "day": T0; "label": T0; "short": T0; "tokens": T0; "input": T0; "output": T0; "cached": T0; "cacheWrite": T0; "turns": T0; "messages": T0; "px": T39; "spx": T39; };
type T41 = { "name": T0; "tokens": T0; "turns": T0; "share": T39; };
type T42 = { "slug": T0; "name": T0; "color": T0; "shape": T0; "tokens": T0; "turns": T0; "messages": T0; "share": T39; };
type T43 = T40[];
type T44 = T41[];
type T45 = T42[];
type T46 = { "ok": T1; "error": T0; "period": T0; "periodLabel": T0; "messages": T0; "attachments": T0; "turns": T0; "tokens": T0; "input": T0; "output": T0; "cached": T0; "cacheWrite": T0; "hit": T0; "sentence": T0; "days": T43; "rows": T43; "models": T44; "bots": T45; "empty": T1; "first": T0; "last": T0; };
type T47 = { "id": T0; "title": T0; "detail": T0; "url": T0; };
type T48 = T47[];
type T49 = { "ok": T1; "total": T39; "pictures": T48; "files": T48; "links": T48; };
type T50 = { "id": T0; "title": T0; "url": T0; };
type T51 = T50[];
type T52 = { "ok": T1; "tabs": T51; "image": T0; "error": T0; "selected": T0; };
type T53 = { "ok": T1; "error": T0; };
type T54 = { "ok": T1; "error": T0; "messageId": T0; };
type T55 = { "ok": T1; "error": T0; "text": T0; };
type T56 = { "authenticated": T1; "passwordRequired": T1; };
type T57 = { "mode": T0; "locked": T1; "restartRequired": T1; "canRestart": T1; "listening": T0; "urls": T0; "error": T0; "http": T1; };
type T58 = { "backend": T0; "root": T0; "backendLock": T0; "rootLock": T0; "restartRequired": T1; "canRestart": T1; "error": T0; "keySet": T1; "space": T0; "spaceLock": T0; };
type T59 = { "ok": T1; "error": T0; "challengeId": T0; };
type T60 = { "ok": T1; "error": T0; "space": T0; "spaceFound": T1; "shared": T1; "schemaReady": T1; };
type T61 = { "name": T0; "about": T0; "color": T0; "shape": T0; "first": T0; };
type T62 = { "ok": T1; "version": T0; "harness": T0; "connected": T1; };
type T63 = { "ok": T1; "installed": T1; "service": T1; "version": T0; "latest": T0; "updateAvailable": T1; "error": T0; };
type T64 = { "ok": T1; "version": T0; "error": T0; };
type T65 = { "ok": T1; "version": T0; };
type T66 = { "ok": T1; "id": T0; "error": T0; };
type T67 = { "userName": T0; "theme": T0; "defaultInstance": T0; "defaultModel": T0; "defaultEffort": T0; "defaultRuntimeMode": T0; "noticeEnabled": T1; "noticeSound": T1; "noticePreview": T0; "noticeWhen": T0; "noticeEnabledMissing": T1; "groupAnswerPolicy": T0; };
type T68 = { "visibilityState": T0; "hasFocus": T1; };
type T69 = { "available": T1; "state": T0; };
type T70 = { "ok": T1; "id": T0; "name": T0; "error": T0; };
type T71 = { "ok": T1; "id": T0; "name": T0; "url": T0; "seconds": T39; "error": T0; };
type T72 = { "ok": T1; "id": T0; "password": T0; "error": T0; };
type T73 = { "ok": T1; "error": T0; "messages": T39; "attachments": T39; "vaultWrites": T39; "iconFetches": T39; };
type T74 = { "stamp": T0; "texts": T5; "files": T5; "voices": T5; };
type T75 = { "locale": T0; };
type T76 = { "hover": T0; };
type T77 = { "accessibilityTextSize": T1; };
type T78 = { "prefersContrast": T0; };
type T79 = { "permanentlyDenied": T1; "shouldShowRationale": T1; };
type T80 = T79 | null;
type T81 = "notDetermined" | "restricted" | "denied" | "authorized";
type T82 = { "authorizationStatus": T81; };
type T83 = T82 | null;
type T84 = "granted" | "prompt" | "denied" | "unknown";
type T85 = { "android": T80; "apple": T83; "available": T1; "state": T84; };
type T86 = T39 | null;
type T87 = { "max": T39; "min": T39; "step": T86; };
type T88 = T0 | null;
type T89 = { "refused": T88; };
type T90 = "audioinput" | "videoinput" | "audiooutput";
type T91 = { "deviceId": T0; "facingMode": T88; "groupId": T0; "kind": T90; "label": T0; };
type T92 = T91[];
type T93 = { "devices": T92; };
type T94 = { "cameraId": T88; "isClientSilenced": T1; "lensFacing": T88; };
type T95 = T94 | null;
type T96 = { "deviceType": T88; "portName": T88; "portType": T88; "position": T88; };
type T97 = T96 | null;
type T98 = T87 | null;
type T99 = T5 | null;
type T100 = T1 | null;
type T101 = { "aspectRatio": T98; "facingMode": T99; "frameRate": T98; "height": T98; "torch": T100; "width": T98; "zoom": T98; };
type T102 = T101 | null;
type T103 = "audio" | "video";
type T104 = "idle" | "interrupted" | "background" | "silenced" | "in-use" | "multitasking" | "system-pressure";
type T105 = T104 | null;
type T106 = "live" | "ended";
type T107 = { "aspectRatio": T86; "autoGainControl": T100; "channelCount": T86; "deviceId": T88; "echoCancellation": T100; "facingMode": T88; "frameRate": T86; "groupId": T88; "height": T86; "latency": T86; "noiseSuppression": T100; "resizeMode": T88; "sampleRate": T86; "sampleSize": T86; "torch": T100; "width": T86; "zoom": T86; };
type T108 = { "android": T95; "apple": T97; "capabilities": T102; "enabled": T1; "id": T0; "kind": T103; "label": T0; "muteReason": T105; "muted": T1; "readyState": T106; "settings": T107; };
type T109 = T108[];
type T110 = { "active": T1; "id": T0; "tracks": T109; };
type T111 = { "aspectRatio": T86; "autoGainControl": T100; "channelCount": T86; "deviceId": T88; "echoCancellation": T100; "facingMode": T88; "frameRate": T86; "groupId": T88; "height": T86; "latency": T86; "noiseSuppression": T100; "resizeMode": T88; "sampleRate": T86; "sampleSize": T86; "torch": T100; "width": T86; "zoom": T86; };
type T112 = { "permanentlyDenied": T1; "shouldShowRationale": T1; };
type T113 = T112 | null;
type T114 = "undetermined" | "denied" | "granted" | "notDetermined" | "restricted" | "authorized";
type T115 = { "recordPermission": T114; };
type T116 = T115 | null;
type T117 = "granted" | "prompt" | "denied" | "unknown";
type T118 = { "android": T113; "apple": T116; "available": T1; "state": T117; };
type T119 = { "x": T39; "y": T39; };
type T120 = T39[];
type T121 = { "audioLevel": T39; "levels": T120; "totalAudioEnergy": T39; "totalSamplesDuration": T39; };
type T122 = "requested" | "interrupted" | "media-reset" | "background" | "track-ended" | "error" | "session-ended";
type T123 = T122 | null;
type T124 = { "duration": T39; "mimeType": T0; "path": T0; "size": T39; };
type T125 = T124 | null;
type T126 = "inactive" | "recording" | "paused";
type T127 = { "audioBitsPerSecond": T86; "duration": T39; "error": T88; "id": T0; "mimeType": T0; "path": T0; "reason": T123; "result": T125; "state": T126; };
type T128 = { "supported": T1; "type": T0; };
type T129 = { "body": T88; "data": T88; "tag": T0; "title": T0; };
type T130 = "notSupported" | "disabled" | "enabled";
type T131 = "notDetermined" | "denied" | "authorized" | "provisional" | "ephemeral";
type T132 = "notSupported" | "disabled" | "enabled";
type T133 = "notSupported" | "disabled" | "enabled";
type T134 = { "alertSetting": T130; "authorizationStatus": T131; "badgeSetting": T132; "soundSetting": T133; };
type T135 = T134 | null;
type T136 = "granted" | "prompt" | "denied" | "unknown";
type T137 = { "apple": T135; "available": T1; "badge": T1; "state": T136; };
type T138 = "mic" | "camcorder" | "voiceRecognition" | "voiceCommunication" | "unprocessed" | "voicePerformance";
type T139 = T138 | null;
type T140 = { "audioSource": T139; "privacySensitive": T100; };
type T141 = "mixWithOthers" | "duckOthers" | "allowBluetoothHFP" | "allowBluetoothA2DP" | "defaultToSpeaker" | "bluetoothHighQualityRecording" | "farFieldInput" | "overrideMutedMicrophoneInterruption";
type T142 = T141[];
type T143 = T142 | null;
type T144 = "builtInWideAngleCamera" | "builtInUltraWideCamera" | "builtInTelephotoCamera" | "builtInDualCamera" | "builtInDualWideCamera" | "builtInTripleCamera" | "builtInTrueDepthCamera" | "builtInLiDARDepthCamera" | "continuityCamera" | "deskViewCamera" | "external";
type T145 = T144 | null;
type T146 = "default" | "measurement" | "voiceChat" | "videoChat" | "videoRecording";
type T147 = T146 | null;
type T148 = "stereo" | "cardioid" | "omnidirectional";
type T149 = T148 | null;
type T150 = { "categoryOptions": T143; "deviceType": T145; "inputDataSource": T88; "mode": T147; "polarPattern": T149; "preferredIOBufferDuration": T86; "prefersNoInterruptionsFromSystemAlerts": T100; };
type T151 = T111 | null;
type T152 = T140 | null;
type T153 = T150 | null;
type T154 = { "audio": T100; "audioConstraints": T151; "video": T100; "videoConstraints": T151; "android": T152; "apple": T153; };
type T155 = "builtInWideAngleCamera" | "builtInUltraWideCamera" | "builtInTelephotoCamera" | "builtInDualCamera" | "builtInDualWideCamera" | "builtInTripleCamera" | "builtInTrueDepthCamera" | "builtInLiDARDepthCamera" | "continuityCamera" | "deskViewCamera" | "external";
type T156 = T155 | null;
type T157 = "locked" | "autoExpose" | "continuousAutoExposure";
type T158 = T157 | null;
type T159 = T119 | null;
type T160 = "locked" | "autoFocus" | "continuousAutoFocus";
type T161 = T160 | null;
type T162 = { "deviceType": T156; "exposureMode": T158; "exposurePointOfInterest": T159; "focusMode": T161; "focusPointOfInterest": T159; };
type T163 = T162 | null;
type T164 = { "aspectRatio": T86; "facingMode": T88; "frameRate": T86; "height": T86; "resizeMode": T88; "torch": T100; "track": T0; "width": T86; "zoom": T86; "apple": T163; };
type T165 = { "track": T0; };
type T166 = { "enabled": T1; "track": T0; };
type T167 = { "track": T0; };
type T168 = "aac" | "heAac" | "opus";
type T169 = T168 | null;
type T170 = "mpeg4" | "ogg" | "webm";
type T171 = T170 | null;
type T172 = { "audioEncoder": T169; "outputFormat": T171; };
type T173 = "min" | "low" | "medium" | "high" | "max";
type T174 = T173 | null;
type T175 = "constant" | "longTermAverage" | "variableConstrained" | "variable";
type T176 = T175 | null;
type T177 = "aac" | "alac" | "linearPCM" | "opus";
type T178 = T177 | null;
type T179 = { "audioQuality": T174; "bitRateStrategy": T176; "formatID": T178; };
type T180 = "constant" | "variable";
type T181 = T180 | null;
type T182 = T172 | null;
type T183 = T179 | null;
type T184 = { "audioBitrateMode": T181; "audioBitsPerSecond": T86; "mimeType": T88; "path": T0; "stream": T0; "android": T182; "apple": T183; };
type T185 = { "recorder": T0; };
type T186 = { "recorder": T0; };
type T187 = { "recorder": T0; };
type T188 = { "interval": T86; "recorder": T0; };
type T189 = { "type": T0; };
type T190 = { "history": T86; "interval": T86; "track": T0; };
type T191 = "passive" | "active" | "timeSensitive";
type T192 = T191 | null;
type T193 = { "interruptionLevel": T192; "relevanceScore": T86; "subtitle": T88; "targetContentIdentifier": T88; "threadIdentifier": T88; };
type T194 = T193 | null;
type T195 = { "body": T88; "data": T88; "navigate": T88; "showTrigger": T86; "silent": T100; "tag": T88; "title": T0; "apple": T194; };
type T196 = { "tag": T0; };
type T197 = { "contents": T86; };
type T198 = { "slug": T0; "id": T0; };
type T199 = { "id": T39; "name": T0; "url": T0; "tab": T0; "params": T198; "fragment": T0; };
type T200 = T199[];
type T201 = { "name": T0; "stack": T200; };
type T202 = T201[];
type T203 = { "tab": T0; "tabs": T202; "next": T39; };
type T204 = T25[];
type T205 = T21[];
type T206 = T2[];
type T207 = T24[];
type T208 = T61[];
type T209 = null;
type T210 = T53 | null;
type T211 = T59 | null;
type T212 = T60 | null;
type T213 = T55 | null;
type T214 = T72 | null;
type T215 = T137 | null;
type T216 = T64 | null;
type T217 = T65 | null;
type T218 = T66 | null;
type T219 = T54 | null;
type T220 = T70 | null;
type T221 = T110 | null;
type T222 = T127 | null;
type T223 = T108 | null;
type T224 = T71 | null;
type T225 = { "key": T0; "shiftKey": T1; "ctrlKey": T1; "altKey": T1; "metaKey": T1; "code": T0; "repeat": T1; "isComposing": T1; };
type T226 = { "offsetX": T39; "offsetY": T39; "button": T39; "buttons": T39; "pressure": T39; "pointerType": T0; "pointerId": T39; "clientX": T39; "clientY": T39; "shiftKey": T1; "ctrlKey": T1; "altKey": T1; "metaKey": T1; };
type T227 = { "offsetX": T39; "offsetY": T39; "deltaX": T39; "deltaY": T39; "deltaMode": T39; "shiftKey": T1; "ctrlKey": T1; "altKey": T1; "metaKey": T1; };
type T228 = { "text": T0; "files": T5; };
type T229 = { "path": T0; "name": T0; "type": T0; "size": T39; "width": T86; "height": T86; "duration": T86; };
type T230 = T229[];
type T231 = { "offsetX": T39; "offsetY": T39; "files": T5; "shiftKey": T1; "ctrlKey": T1; "altKey": T1; "metaKey": T1; };
type T232 = { "scrollLeft": T39; "scrollTop": T39; "scrollWidth": T39; "scrollHeight": T39; "clientWidth": T39; "clientHeight": T39; "firstVisibleKey": T88; "firstVisibleOffset": T39; };
type T233 = { "target": T0; "relatedTarget": T88; };

/** Sources called by compiled mutation sends. */
export type MutationSource = "checkAnytype" | "closeComputer" | "computerControl" | "computerInput" | "confirmPairing" | "copyStorage" | "createBot" | "createGroup" | "createSubagent" | "deleteBot" | "deleteGroup" | "deleteLogin" | "dropVoice" | "enableNotifications" | "hideBot" | "makeMainBot" | "markRoomSeen" | "markSeen" | "noteUnread" | "openComputer" | "pinBot" | "pinMessage" | "prepareAnytype" | "reactTo" | "readHealth" | "recolourBot" | "renameBot" | "reshapeBot" | "respondRequest" | "restartGateway" | "restartStorage" | "revealPassword" | "saveBotPrompt" | "saveDrafts" | "saveLogin" | "saveSpace" | "saveStorage" | "saveSubagent" | "sendMessage" | "sendRoom" | "sendVoice" | "setBotNotifications" | "setDefaultMode" | "setDefaultModel" | "setGroupAnswerPolicy" | "setModel" | "setNetwork" | "setNotifications" | "setPassword" | "setTheme" | "setUserName" | "signIn" | "signOut" | "stageVoice" | "startPairing" | "startUpdate" | "stopRun" | "testStorage" | "updateGroup" | "uploadAttachment";

export interface SourceMap {
  "checkAnytype": { args: []; result: T60 };
  "closeComputer": { args: [T0]; result: T53 };
  "computerControl": { args: [T0, T0, T0]; result: T53 };
  "computerInput": { args: [T0, T0, T39, T39, T39, T39, T39, T0, T39, T39]; result: T55 };
  "confirmPairing": { args: [T0, T0]; result: T53 };
  "copyStorage": { args: [T0]; result: T53 };
  "createBot": { args: [T0, T0, T0, T0, T0]; result: T66 };
  "createGroup": { args: [T0, T0, T0, T0, T0, T0, T0]; result: T66 };
  "createSubagent": { args: [T0, T0, T0, T0, T0, T1]; result: T53 };
  "deleteBot": { args: [T0]; result: T53 };
  "deleteGroup": { args: [T0]; result: T53 };
  "deleteLogin": { args: [T0]; result: T53 };
  "dropVoice": { args: [T0, T0]; result: T53 };
  "enableNotifications": { args: []; result: T53 };
  "hideBot": { args: [T0, T1]; result: T53 };
  "loadAbout": { args: []; result: T62 };
  "loadBots": { args: [T0]; result: T206 };
  "loadChat": { args: [T0, T0]; result: T15 };
  "loadComputer": { args: [T0, T0, T0, T0, T0]; result: T52 };
  "loadConvos": { args: [T0, T0, T0]; result: T207 };
  "loadDrafts": { args: []; result: T74 };
  "loadLineage": { args: [T0, T0, T0]; result: T38 };
  "loadModels": { args: [T0, T0, T0]; result: T32 };
  "loadNetwork": { args: [T0]; result: T57 };
  "loadNotice": { args: []; result: T69 };
  "loadPrefs": { args: []; result: T67 };
  "loadProviders": { args: []; result: T31 };
  "loadRoom": { args: [T0]; result: T20 };
  "loadRoutines": { args: [T0, T0, T0]; result: T35 };
  "loadSession": { args: [T0]; result: T56 };
  "loadShelf": { args: [T0, T0]; result: T49 };
  "loadStorage": { args: [T0]; result: T58 };
  "loadSuggestions": { args: []; result: T208 };
  "loadSystem": { args: [T0]; result: T63 };
  "loadUsage": { args: [T0, T0]; result: T73 };
  "loadUsageReport": { args: [T0, T0, T0, T0]; result: T46 };
  "loadVault": { args: []; result: T204 };
  "makeMainBot": { args: [T0]; result: T53 };
  "markRoomSeen": { args: [T0]; result: T53 };
  "markSeen": { args: [T0]; result: T53 };
  "noteUnread": { args: [T0, T1, T1, T0, T1, T1, T1, T0, T0, T0, T0]; result: T53 };
  "openComputer": { args: [T0]; result: T53 };
  "pinBot": { args: [T0, T1]; result: T53 };
  "pinMessage": { args: [T0, T0, T0, T0]; result: T53 };
  "prepareAnytype": { args: []; result: T53 };
  "reactTo": { args: [T0, T0, T0]; result: T53 };
  "readHealth": { args: []; result: T65 };
  "recolourBot": { args: [T0, T0]; result: T53 };
  "renameBot": { args: [T0, T0]; result: T53 };
  "reshapeBot": { args: [T0, T0]; result: T53 };
  "respondRequest": { args: [T0, T0, T0, T0, T0, T0]; result: T53 };
  "restartGateway": { args: []; result: T53 };
  "restartStorage": { args: []; result: T53 };
  "revealPassword": { args: [T0]; result: T72 };
  "saveBotPrompt": { args: [T0, T0]; result: T53 };
  "saveDrafts": { args: [T5, T5, T5]; result: T53 };
  "saveLogin": { args: [T0, T0, T0, T0, T0, T0]; result: T53 };
  "saveSpace": { args: [T0]; result: T53 };
  "saveStorage": { args: [T0, T0, T0]; result: T53 };
  "saveSubagent": { args: [T0, T0, T0, T0, T0, T1]; result: T53 };
  "sendMessage": { args: [T0, T0, T0, T0, T0, T0, T0]; result: T54 };
  "sendRoom": { args: [T0, T0, T0]; result: T54 };
  "sendVoice": { args: [T0, T39, T0, T0]; result: T53 };
  "setBotNotifications": { args: [T0, T1]; result: T53 };
  "setDefaultMode": { args: [T0]; result: T53 };
  "setDefaultModel": { args: [T0, T0, T0]; result: T53 };
  "setGroupAnswerPolicy": { args: [T0]; result: T53 };
  "setModel": { args: [T0, T0, T0, T0, T0]; result: T53 };
  "setNetwork": { args: [T0]; result: T53 };
  "setNotifications": { args: [T0, T1, T0, T0]; result: T53 };
  "setPassword": { args: [T0]; result: T53 };
  "setTheme": { args: [T0]; result: T53 };
  "setUserName": { args: [T0]; result: T53 };
  "signIn": { args: [T0]; result: T53 };
  "signOut": { args: []; result: T53 };
  "stageVoice": { args: [T0, T0, T0, T39]; result: T71 };
  "startPairing": { args: []; result: T59 };
  "startUpdate": { args: []; result: T64 };
  "stopRun": { args: [T0, T0]; result: T53 };
  "testStorage": { args: [T0]; result: T53 };
  "updateGroup": { args: [T0, T0, T0]; result: T53 };
  "uploadAttachment": { args: [T0, T0, T0, T0, T0]; result: T70 };
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
