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
//   createManagementMcpCredential: (args: [string, string]) => McpResult
//   createSubagent: (args: [string, string, string, string, string, boolean]) => Reply
//   deleteBot: (args: [string]) => Reply
//   deleteGroup: (args: [string]) => Reply
//   deleteLogin: (args: [string]) => Reply
//   dropVoice: (args: [string, string]) => Reply
//   enableNotifications: (args: []) => Reply
//   hideBot: (args: [string, boolean]) => Reply
//   loadAbout: (args: []) => About
//   loadBots: (args: [string]) => Bot[]
//   loadChat: (args: [string, string, string]) => Chat
//   loadComputer: (args: [string, string, string, string, string]) => Computer
//   loadComputerPlace: (args: [string, string, string]) => ComputerPlace
//   loadConvos: (args: [string, string, string]) => Convo[]
//   loadDrafts: (args: []) => DraftPack
//   loadLineage: (args: [string, string, string]) => Lineage
//   loadManagementMcp: (args: [string]) => McpStatus
//   loadMemory: (args: [string, string]) => MemoryFile
//   loadModels: (args: [string, string, string]) => ModelList
//   loadNetwork: (args: [string]) => Network
//   loadNotice: (args: []) => NoticeState
//   loadPageOrigin: (args: []) => PageOrigin
//   loadPrefs: (args: []) => Prefs
//   loadProviders: (args: []) => Provider[]
//   loadRoom: (args: [string]) => Room
//   loadRoutines: (args: [string, string, string]) => RoutineList
//   loadSession: (args: [string]) => Session
//   loadShelf: (args: [string, string]) => Shelf
//   loadSideChats: (args: [string]) => SideList
//   loadStorage: (args: [string]) => Storage
//   loadSuggestions: (args: []) => Suggestion[]
//   loadSystem: (args: [string]) => SystemStatus
//   loadUsage: (args: [string, string]) => Usage
//   loadUsageReport: (args: [string, string, string, string]) => UsageReport
//   loadVault: (args: []) => Login[]
//   makeMainBot: (args: [string]) => Reply
//   markRoomSeen: (args: [string]) => Reply
//   markSeen: (args: [string]) => Reply
//   mergeSide: (args: [string, string]) => Reply
//   noteUnread: (args: [string, boolean, boolean, string, boolean, boolean, boolean, string, string, string, string]) => Reply
//   openComputer: (args: [string]) => Reply
//   openSide: (args: [string, string]) => SideOpen
//   pinBot: (args: [string, boolean]) => Reply
//   pinMessage: (args: [string, string, string, string]) => Reply
//   prepareAnytype: (args: []) => Reply
//   reactTo: (args: [string, string, string, string]) => Reply
//   readHealth: (args: []) => Health
//   readPromptLink: (args: [string]) => PromptLink
//   recolourBot: (args: [string, string]) => Reply
//   renameBot: (args: [string, string]) => Reply
//   reshapeBot: (args: [string, string]) => Reply
//   respondRequest: (args: [string, string, string, string, string, string, string]) => Reply
//   restartGateway: (args: []) => Reply
//   restartStorage: (args: []) => Reply
//   revealPassword: (args: [string]) => Revealed
//   revokeManagementMcpCredential: (args: [string]) => McpResult
//   saveBotMemory: (args: [string, string, string]) => MemorySave
//   saveBotPrompt: (args: [string, string]) => Reply
//   saveDrafts: (args: [string[], string[], string[]]) => Reply
//   saveLogin: (args: [string, string, string, string, string, string]) => Reply
//   saveSpace: (args: [string]) => Reply
//   saveStorage: (args: [string, string, string]) => Reply
//   saveSubagent: (args: [string, string, string, string, string, boolean]) => Reply
//   sendMessage: (args: [string, string, string, string, string, string, string, string]) => Sent
//   sendRoom: (args: [string, string, string]) => Sent
//   sendVoice: (args: [string, number, string, string, string]) => Reply
//   setBotNotifications: (args: [string, boolean]) => Reply
//   setComputerPlace: (args: [string, string]) => Reply
//   setDefaultMode: (args: [string]) => Reply
//   setDefaultModel: (args: [string, string, string]) => Reply
//   setGroupAnswerPolicy: (args: [string]) => Reply
//   setManagementMcp: (args: [boolean]) => McpResult
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
//   stopRun: (args: [string, string, string]) => Reply
//   testStorage: (args: [string]) => Reply
//   updateGroup: (args: [string, string, string]) => Reply
//   uploadAttachment: (args: [string, string, string, string, string]) => Attached
// The shapes. None is exported: name one by its source, `type AnytypeCheck = Result<'checkAnytype'>`.
//   Bot = { slug: string; name: string; label: string; line: string; time: string; initials: string; color: string; shape: string; main: boolean; pinned: boolean; hidden: boolean; ready: boolean; working: boolean; unread: boolean; parentSlug: string; hideFromSidebar: boolean; subagent: boolean; prompt: string; inheritParentPrompt: boolean; notifications: boolean; aboveLimit: boolean }
//   FileRef = { id: string; name: string; kind: string; size: string; url: string; width: number; height: number }
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
//   SideRef = { id: string; title: string }
//   SideList = { chats: SideRef[] }
//   SideOpen = { ok: boolean; error: string; threadId: string; messageId: string }
//   McpCredential = { id: string; label: string; permissions: string }
//   McpStatus = { loaded: boolean; enabled: boolean; endpoint: string; credentials: McpCredential[] }
//   McpResult = { ok: boolean; error: string; token: string }
//   MemoryFile = { ok: boolean; path: string; text: string; revision: string; truncated: boolean }
//   MemorySave = { ok: boolean; error: string; conflict: boolean; text: string; revision: string }
//   ComputerLink = { id: string; label: string; status: string; access: string; selectable: boolean }
//   ComputerPlace = { ok: boolean; selected: string; localLabel: string; links: ComputerLink[]; waiting: boolean; error: string }
//   PromptLink = { bot: string; prompt: string; present: boolean; bad: boolean }
//   PageOrigin = { origin: string; local: boolean }

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
type T1 = T0 | null;
type T2 = { "body": T1; "data": T1; "tag": T0; "title": T0; };
type T3 = "notSupported" | "disabled" | "enabled";
type T4 = "notDetermined" | "denied" | "authorized" | "provisional" | "ephemeral";
type T5 = "notSupported" | "disabled" | "enabled";
type T6 = "notSupported" | "disabled" | "enabled";
type T7 = { "alertSetting": T3; "authorizationStatus": T4; "badgeSetting": T5; "soundSetting": T6; };
type T8 = T7 | null;
type T9 = boolean;
type T10 = "granted" | "prompt" | "denied" | "unknown";
type T11 = { "apple": T8; "available": T9; "badge": T9; "state": T10; };
type T12 = { "slug": T0; "name": T0; "label": T0; "line": T0; "time": T0; "initials": T0; "color": T0; "shape": T0; "main": T9; "pinned": T9; "hidden": T9; "ready": T9; "working": T9; "unread": T9; "parentSlug": T0; "hideFromSidebar": T9; "subagent": T9; "prompt": T0; "inheritParentPrompt": T9; "notifications": T9; "aboveLimit": T9; };
type T13 = number;
type T14 = { "id": T0; "name": T0; "kind": T0; "size": T0; "url": T0; "width": T13; "height": T13; };
type T15 = { "label": T0; "value": T0; "key": T0; "warn": T0; };
type T16 = T0[];
type T17 = T16[];
type T18 = { "kind": T0; "text": T0; "head": T16; "rows": T17; "align": T16; };
type T19 = T14[];
type T20 = T15[];
type T21 = T18[];
type T22 = { "id": T0; "kind": T0; "mine": T9; "text": T0; "title": T0; "time": T0; "head": T9; "tail": T9; "variant": T0; "files": T19; "botSlug": T0; "botName": T0; "botColor": T0; "botShape": T0; "choices": T20; "placeholder": T0; "key": T0; "react": T0; "quote": T0; "tokens": T0; "resetAt": T0; "parts": T21; };
type T23 = { "id": T0; "text": T0; "missing": T9; };
type T24 = T22[];
type T25 = T23[];
type T26 = { "slug": T0; "rows": T24; "working": T9; "partial": T9; "readOnly": T9; "pins": T25; "threadId": T0; };
type T27 = { "slug": T0; "name": T0; "line": T0; "state": T0; "ready": T9; "working": T9; };
type T28 = { "id": T0; "name": T0; "text": T0; "from": T0; "parts": T21; };
type T29 = T27[];
type T30 = T28[];
type T31 = { "id": T0; "title": T0; "description": T0; "color": T0; "shape": T0; "members": T29; "messages": T30; "notifications": T9; "answerPolicy": T0; "answerPolicyEffective": T0; "chief": T0; };
type T32 = { "slug": T0; "name": T0; "color": T0; "shape": T0; };
type T33 = { "shape": T0; "color": T0; "state": T0; };
type T34 = T33[];
type T35 = { "kind": T0; "key": T0; "id": T0; "name": T0; "color": T0; "shape": T0; "faces": T34; "line": T0; "time": T0; "at": T0; "unread": T9; "working": T9; "ready": T9; "notifications": T9; };
type T36 = { "id": T0; "title": T0; "website": T0; "username": T0; "hasPassword": T9; "hints": T0; "icon": T0; };
type T37 = { "id": T0; "label": T0; };
type T38 = T37[];
type T39 = { "slug": T0; "name": T0; "recommended": T9; "effort": T0; "efforts": T38; };
type T40 = T39[];
type T41 = { "instanceId": T0; "name": T0; "account": T0; "models": T40; };
type T42 = T41[];
type T43 = { "ok": T9; "error": T0; "current": T0; "instanceId": T0; "effort": T0; "runtimeMode": T0; "currentName": T0; "providerName": T0; "effortLabel": T0; "efforts": T38; "providers": T42; };
type T44 = { "id": T0; "title": T0; "paused": T9; "schedule": T0; "next": T0; "failed": T9; };
type T45 = T44[];
type T46 = { "ok": T9; "tasks": T45; "error": T0; };
type T47 = { "id": T0; "title": T0; "status": T0; "remote": T9; "threadId": T0; };
type T48 = T47[];
type T49 = { "ok": T9; "rows": T48; "error": T0; };
type T50 = { "day": T0; "label": T0; "short": T0; "tokens": T0; "input": T0; "output": T0; "cached": T0; "cacheWrite": T0; "turns": T0; "messages": T0; "px": T13; "spx": T13; };
type T51 = { "name": T0; "tokens": T0; "turns": T0; "share": T13; };
type T52 = { "slug": T0; "name": T0; "color": T0; "shape": T0; "tokens": T0; "turns": T0; "messages": T0; "share": T13; };
type T53 = T50[];
type T54 = T51[];
type T55 = T52[];
type T56 = { "ok": T9; "error": T0; "period": T0; "periodLabel": T0; "messages": T0; "attachments": T0; "turns": T0; "tokens": T0; "input": T0; "output": T0; "cached": T0; "cacheWrite": T0; "hit": T0; "sentence": T0; "days": T53; "rows": T53; "models": T54; "bots": T55; "empty": T9; "first": T0; "last": T0; };
type T57 = { "id": T0; "title": T0; "detail": T0; "url": T0; };
type T58 = T57[];
type T59 = { "ok": T9; "total": T13; "pictures": T58; "files": T58; "links": T58; };
type T60 = { "id": T0; "title": T0; "url": T0; };
type T61 = T60[];
type T62 = { "ok": T9; "tabs": T61; "image": T0; "error": T0; "selected": T0; };
type T63 = { "ok": T9; "error": T0; };
type T64 = { "ok": T9; "error": T0; "messageId": T0; };
type T65 = { "ok": T9; "error": T0; "text": T0; };
type T66 = { "authenticated": T9; "passwordRequired": T9; };
type T67 = { "mode": T0; "locked": T9; "restartRequired": T9; "canRestart": T9; "listening": T0; "urls": T0; "error": T0; "http": T9; };
type T68 = { "backend": T0; "root": T0; "backendLock": T0; "rootLock": T0; "restartRequired": T9; "canRestart": T9; "error": T0; "keySet": T9; "space": T0; "spaceLock": T0; };
type T69 = { "ok": T9; "error": T0; "challengeId": T0; };
type T70 = { "ok": T9; "error": T0; "space": T0; "spaceFound": T9; "shared": T9; "schemaReady": T9; };
type T71 = { "name": T0; "about": T0; "color": T0; "shape": T0; "first": T0; };
type T72 = { "ok": T9; "version": T0; "harness": T0; "connected": T9; };
type T73 = { "ok": T9; "installed": T9; "service": T9; "version": T0; "latest": T0; "updateAvailable": T9; "error": T0; };
type T74 = { "ok": T9; "version": T0; "error": T0; };
type T75 = { "ok": T9; "version": T0; };
type T76 = { "ok": T9; "id": T0; "error": T0; };
type T77 = { "userName": T0; "theme": T0; "defaultInstance": T0; "defaultModel": T0; "defaultEffort": T0; "defaultRuntimeMode": T0; "noticeEnabled": T9; "noticeSound": T9; "noticePreview": T0; "noticeWhen": T0; "noticeEnabledMissing": T9; "groupAnswerPolicy": T0; };
type T78 = { "visibilityState": T0; "hasFocus": T9; };
type T79 = { "available": T9; "state": T0; };
type T80 = { "ok": T9; "id": T0; "name": T0; "error": T0; };
type T81 = { "ok": T9; "id": T0; "name": T0; "url": T0; "seconds": T13; "error": T0; };
type T82 = { "ok": T9; "id": T0; "password": T0; "error": T0; };
type T83 = { "ok": T9; "error": T0; "messages": T13; "attachments": T13; "vaultWrites": T13; "iconFetches": T13; };
type T84 = { "stamp": T0; "texts": T16; "files": T16; "voices": T16; };
type T85 = { "locale": T0; };
type T86 = { "hover": T0; };
type T87 = { "id": T0; "title": T0; };
type T88 = T87[];
type T89 = { "chats": T88; };
type T90 = { "ok": T9; "error": T0; "threadId": T0; "messageId": T0; };
type T91 = { "id": T0; "label": T0; "permissions": T0; };
type T92 = T91[];
type T93 = { "loaded": T9; "enabled": T9; "endpoint": T0; "credentials": T92; };
type T94 = { "ok": T9; "error": T0; "token": T0; };
type T95 = T70 | null;
type T96 = T12[];
type T97 = T35[];
type T98 = T63 | null;
type T99 = T11 | null;
type T100 = T69 | null;
type T101 = T74 | null;
type T102 = { "mcpStatus": T93; "mcpToken": T0; "mcpError": T0; "mcpPending": T9; "about": T72; "anytypeChecked": T95; "anytypePrepareError": T0; "anytypePreparedPending": T9; "anytypeError": T0; "anytypeFound": T9; "anytypeOn": T9; "anytypeShared": T9; "anytypeSpace": T0; "bots": T96; "canUpdate": T9; "convos": T97; "nameDraft": T0; "nameEditing": T9; "network": T67; "networkError": T0; "networkRestartError": T0; "networkRestarted": T98; "networkSet": T98; "newPassword": T0; "notice": T79; "noticeAsked": T99; "pairCode": T0; "pairConfirmError": T0; "pairConfirmed": T98; "pairError": T0; "pairStarted": T100; "pairStep": T0; "passwordError": T0; "passwordSet": T98; "prefs": T77; "prefsError": T0; "providers": T42; "report": T56; "reportFailed": T9; "session": T66; "settingsDomain": T0; "shownBackend": T0; "spaceEditing": T9; "spaceName": T0; "spaceSaveError": T0; "spaceSaved": T98; "storage": T68; "storageCopied": T98; "storageCopyError": T0; "storageHint": T0; "storagePath": T0; "storageRestartError": T0; "storageRestarted": T98; "storageSaveError": T0; "storageSaved": T98; "storageTestError": T0; "storageTested": T98; "system": T73; "updateError": T0; "updateFrom": T0; "updateNeedsService": T9; "updateStarted": T101; "updateStuck": T9; "updateTo": T0; "usagePeriod": T0; "usageView": T0; "anytypeCheckedPending": T9; "networkRestartedPending": T9; "networkSetPending": T9; "noticeAskedPending": T9; "pairConfirmedPending": T9; "pairStartedPending": T9; "passwordSetPending": T9; "spaceSavedPending": T9; "storageCopiedPending": T9; "storageRestartedPending": T9; "storageSavedPending": T9; "storageTestedPending": T9; "systemPending": T9; "updateStartedPending": T9; };
type T103 = T76 | null;
type T104 = { "bots": T96; "canMakeGroup": T9; "groupAbout": T0; "groupChief": T0; "groupError": T0; "groupMade": T103; "groupMembers": T16; "groupName": T0; "groupPolicy": T0; "groupStep": T0; "pickableBots": T96; "groupMadePending": T9; };
type T105 = T71[];
type T106 = { "createError": T0; "createStep": T0; "createdPending": T9; "newColor": T0; "newFirst": T0; "newName": T0; "newPrompt": T0; "newShape": T0; "suggestions": T105; };
type T107 = { "accessibilityTextSize": T9; };
type T108 = { "prefersContrast": T0; };
type T109 = { "key": T0; "kind": T0; "text": T0; "slug": T0; "name": T0; "color": T0; "shape": T0; };
type T110 = { "key": T0; "text": T0; "start": T13; "n": T13; };
type T111 = { "start": T13; "query": T0; };
type T112 = { "text": T0; "cursor": T13; };
type T113 = { "start": T13; "end": T13; "slug": T0; "kind": T0; "name": T0; "color": T0; "shape": T0; };
type T114 = T32[];
type T115 = T64 | null;
type T116 = { "answerError": T0; "answerText": T0; "answered": T98; "answeredPending": T9; "away": T9; "bots": T96; "chat": T26; "copiedMenu": T0; "groupPage": T0; "groupPanel": T9; "groupSaveError": T0; "mentionHits": T114; "mentionIdx": T13; "messageMenu": T0; "paintedWorkers": T29; "pointerFacts": T86; "prefs": T77; "reactFor": T0; "room": T31; "roomError": T0; "roomPending": T0; "roomRows": T24; "roomSent": T115; "roomText": T0; "wide": T9; "roomSentPending": T9; "roomFailed": T9; };
type T117 = { "toast": T0; };
type T118 = { "computerEnlarged": T9; "computer": T62; "onComputer": T9; "holding": T9; "locked": T9; "bots": T96; "slug": T0; };
type T119 = { "system": T73; "password": T0; "signedIn": T98; "signInError": T0; "locked": T9; "signedInPending": T9; };
type T120 = { "rows": T24; "working": T9; "draft": T0; "canSend": T9; "pending": T16; "attachError": T0; "attachedPending": T9; "sentPending": T9; "replyTo": T0; "replyQuote": T0; "voicePreview": T0; "voiceError": T0; "voiceHold": T0; "voiceId": T0; "recording": T9; "voiceSentPending": T9; "answerText": T0; "answerError": T0; "answering": T9; "canMerge": T9; "name": T0; "bots": T96; "slug": T0; "pins": T25; "hover": T9; "reactFor": T0; "messageMenu": T0; "copiedMenu": T0; "empty": T9; "failed": T9; "loading": T9; };
type T121 = { "ok": T9; "path": T0; "text": T0; "revision": T0; "truncated": T9; };
type T122 = { "ok": T9; "error": T0; "conflict": T9; "text": T0; "revision": T0; };
type T123 = { "file": T121; "failed": T9; "pending": T9; "editing": T9; "draft": T0; "conflict": T9; "serverText": T0; "serverRevision": T0; "error": T0; "saving": T9; };
type T124 = { "id": T0; "label": T0; "status": T0; "access": T0; "selectable": T9; };
type T125 = T124[];
type T126 = { "ok": T9; "selected": T0; "localLabel": T0; "links": T125; "waiting": T9; "error": T0; };
type T127 = T80 | null;
type T128 = { "system": T73; "models": T43; "modelError": T0; "shownRows": T24; "searchNote": T0; "away": T9; "answerText": T0; "chatSearching": T9; "messageQuery": T0; "reactFor": T0; "messageMenu": T0; "copiedMenu": T0; "replyTo": T0; "replyQuote": T0; "childThread": T0; "sent": T115; "answered": T98; "attached": T127; "pendingText": T0; "attachError": T0; "answerError": T0; "paintedWorkers": T29; "draft": T0; "mentionIdx": T13; "mentionHits": T114; "usagePeriod": T0; "usageView": T0; "report": T56; "childModels": T43; "routines": T46; "lineage": T49; "shelf": T59; "voiceRecorder": T0; "voiceError": T0; "voiceSent": T98; "voiceHold": T0; "voiceId": T0; "voicePreview": T0; "wide": T9; "panelOpen": T9; "panelTab": T0; "panelPage": T0; "subSlug": T0; "subName": T0; "subShape": T0; "subColor": T0; "subPrompt": T0; "subInherit": T9; "subCreated": T98; "subSaved": T98; "botError": T0; "subError": T0; "subSaveError": T0; "childRemoveError": T0; "heldTab": T0; "computerTab": T0; "computer": T62; "controlError": T0; "openError": T0; "openingBrowser": T9; "closeError": T0; "closingBrowser": T9; "driveError": T0; "holding": T9; "pointerFacts": T86; "bots": T96; "page": T78; "chat": T26; "slug": T0; "pendingAttaches": T16; "canSend": T9; "chatFailed": T9; "chatPending": T9; "answeredPending": T9; "attachedPending": T9; "voiceSentPending": T9; "sentPending": T9; "modelsFailed": T9; "routinesFailed": T9; "lineageFailed": T9; "reportFailed": T9; "shelfFailed": T9; "computerFailed": T9; "subCreatedPending": T9; "subSavedPending": T9; "childModelsFailed": T9; "linkDraft": T9; "linkLocal": T9; };
type T129 = { "prefs": T77; "botQuery": T0; "searching": T9; "hiddenOpen": T9; "rowMenu": T0; "firstRun": T9; "visibleBots": T96; "tiles": T96; "hiddenBots": T96; "newColor": T0; "newShape": T0; "wide": T9; "plusOpen": T9; "bots": T96; "convos": T97; "chat": T26; "slug": T0; "query": T0; "roomId": T0; "convoName": T0; "botsFailed": T9; };
type T130 = T36[];
type T131 = { "vaultQuery": T0; "vault": T130; "vquery": T0; "shownVault": T130; "query": T0; "vaultFailed": T9; "vaultPending": T9; };
type T132 = { "permanentlyDenied": T9; "shouldShowRationale": T9; };
type T133 = T132 | null;
type T134 = "notDetermined" | "restricted" | "denied" | "authorized";
type T135 = { "authorizationStatus": T134; };
type T136 = T135 | null;
type T137 = "granted" | "prompt" | "denied" | "unknown";
type T138 = { "android": T133; "apple": T136; "available": T9; "state": T137; };
type T139 = T13 | null;
type T140 = { "max": T13; "min": T13; "step": T139; };
type T141 = { "refused": T1; };
type T142 = "audioinput" | "videoinput" | "audiooutput";
type T143 = { "deviceId": T0; "facingMode": T1; "groupId": T0; "kind": T142; "label": T0; };
type T144 = T143[];
type T145 = { "devices": T144; };
type T146 = { "cameraId": T1; "isClientSilenced": T9; "lensFacing": T1; };
type T147 = T146 | null;
type T148 = { "deviceType": T1; "portName": T1; "portType": T1; "position": T1; };
type T149 = T148 | null;
type T150 = T140 | null;
type T151 = T16 | null;
type T152 = T9 | null;
type T153 = { "aspectRatio": T150; "facingMode": T151; "frameRate": T150; "height": T150; "torch": T152; "width": T150; "zoom": T150; };
type T154 = T153 | null;
type T155 = "audio" | "video";
type T156 = "idle" | "interrupted" | "background" | "silenced" | "in-use" | "multitasking" | "system-pressure";
type T157 = T156 | null;
type T158 = "live" | "ended";
type T159 = { "aspectRatio": T139; "autoGainControl": T152; "channelCount": T139; "deviceId": T1; "echoCancellation": T152; "facingMode": T1; "frameRate": T139; "groupId": T1; "height": T139; "latency": T139; "noiseSuppression": T152; "resizeMode": T1; "sampleRate": T139; "sampleSize": T139; "torch": T152; "width": T139; "zoom": T139; };
type T160 = { "android": T147; "apple": T149; "capabilities": T154; "enabled": T9; "id": T0; "kind": T155; "label": T0; "muteReason": T157; "muted": T9; "readyState": T158; "settings": T159; };
type T161 = T160[];
type T162 = { "active": T9; "id": T0; "tracks": T161; };
type T163 = { "aspectRatio": T139; "autoGainControl": T152; "channelCount": T139; "deviceId": T1; "echoCancellation": T152; "facingMode": T1; "frameRate": T139; "groupId": T1; "height": T139; "latency": T139; "noiseSuppression": T152; "resizeMode": T1; "sampleRate": T139; "sampleSize": T139; "torch": T152; "width": T139; "zoom": T139; };
type T164 = { "permanentlyDenied": T9; "shouldShowRationale": T9; };
type T165 = T164 | null;
type T166 = "undetermined" | "denied" | "granted" | "notDetermined" | "restricted" | "authorized";
type T167 = { "recordPermission": T166; };
type T168 = T167 | null;
type T169 = "granted" | "prompt" | "denied" | "unknown";
type T170 = { "android": T165; "apple": T168; "available": T9; "state": T169; };
type T171 = { "x": T13; "y": T13; };
type T172 = T13[];
type T173 = { "audioLevel": T13; "levels": T172; "totalAudioEnergy": T13; "totalSamplesDuration": T13; };
type T174 = "requested" | "interrupted" | "media-reset" | "background" | "track-ended" | "error" | "session-ended";
type T175 = T174 | null;
type T176 = { "duration": T13; "mimeType": T0; "path": T0; "size": T13; };
type T177 = T176 | null;
type T178 = "inactive" | "recording" | "paused";
type T179 = { "audioBitsPerSecond": T139; "duration": T13; "error": T1; "id": T0; "mimeType": T0; "path": T0; "reason": T175; "result": T177; "state": T178; };
type T180 = { "supported": T9; "type": T0; };
type T181 = { "bot": T0; "prompt": T0; "present": T9; "bad": T9; };
type T182 = { "origin": T0; "local": T9; };
type T183 = "passive" | "active" | "timeSensitive";
type T184 = T183 | null;
type T185 = { "interruptionLevel": T184; "relevanceScore": T139; "subtitle": T1; "targetContentIdentifier": T1; "threadIdentifier": T1; };
type T186 = T185 | null;
type T187 = { "body": T1; "data": T1; "navigate": T1; "showTrigger": T139; "silent": T152; "tag": T1; "title": T0; "apple": T186; };
type T188 = { "tag": T0; };
type T189 = { "contents": T139; };
type T190 = "mic" | "camcorder" | "voiceRecognition" | "voiceCommunication" | "unprocessed" | "voicePerformance";
type T191 = T190 | null;
type T192 = { "audioSource": T191; "privacySensitive": T152; };
type T193 = "mixWithOthers" | "duckOthers" | "allowBluetoothHFP" | "allowBluetoothA2DP" | "defaultToSpeaker" | "bluetoothHighQualityRecording" | "farFieldInput" | "overrideMutedMicrophoneInterruption";
type T194 = T193[];
type T195 = T194 | null;
type T196 = "builtInWideAngleCamera" | "builtInUltraWideCamera" | "builtInTelephotoCamera" | "builtInDualCamera" | "builtInDualWideCamera" | "builtInTripleCamera" | "builtInTrueDepthCamera" | "builtInLiDARDepthCamera" | "continuityCamera" | "deskViewCamera" | "external";
type T197 = T196 | null;
type T198 = "default" | "measurement" | "voiceChat" | "videoChat" | "videoRecording";
type T199 = T198 | null;
type T200 = "stereo" | "cardioid" | "omnidirectional";
type T201 = T200 | null;
type T202 = { "categoryOptions": T195; "deviceType": T197; "inputDataSource": T1; "mode": T199; "polarPattern": T201; "preferredIOBufferDuration": T139; "prefersNoInterruptionsFromSystemAlerts": T152; };
type T203 = T163 | null;
type T204 = T192 | null;
type T205 = T202 | null;
type T206 = { "audio": T152; "audioConstraints": T203; "video": T152; "videoConstraints": T203; "android": T204; "apple": T205; };
type T207 = "builtInWideAngleCamera" | "builtInUltraWideCamera" | "builtInTelephotoCamera" | "builtInDualCamera" | "builtInDualWideCamera" | "builtInTripleCamera" | "builtInTrueDepthCamera" | "builtInLiDARDepthCamera" | "continuityCamera" | "deskViewCamera" | "external";
type T208 = T207 | null;
type T209 = "locked" | "autoExpose" | "continuousAutoExposure";
type T210 = T209 | null;
type T211 = T171 | null;
type T212 = "locked" | "autoFocus" | "continuousAutoFocus";
type T213 = T212 | null;
type T214 = { "deviceType": T208; "exposureMode": T210; "exposurePointOfInterest": T211; "focusMode": T213; "focusPointOfInterest": T211; };
type T215 = T214 | null;
type T216 = { "aspectRatio": T139; "facingMode": T1; "frameRate": T139; "height": T139; "resizeMode": T1; "torch": T152; "track": T0; "width": T139; "zoom": T139; "apple": T215; };
type T217 = { "track": T0; };
type T218 = { "enabled": T9; "track": T0; };
type T219 = { "track": T0; };
type T220 = "aac" | "heAac" | "opus";
type T221 = T220 | null;
type T222 = "mpeg4" | "ogg" | "webm";
type T223 = T222 | null;
type T224 = { "audioEncoder": T221; "outputFormat": T223; };
type T225 = "min" | "low" | "medium" | "high" | "max";
type T226 = T225 | null;
type T227 = "constant" | "longTermAverage" | "variableConstrained" | "variable";
type T228 = T227 | null;
type T229 = "aac" | "alac" | "linearPCM" | "opus";
type T230 = T229 | null;
type T231 = { "audioQuality": T226; "bitRateStrategy": T228; "formatID": T230; };
type T232 = "constant" | "variable";
type T233 = T232 | null;
type T234 = T224 | null;
type T235 = T231 | null;
type T236 = { "audioBitrateMode": T233; "audioBitsPerSecond": T139; "mimeType": T1; "path": T0; "stream": T0; "android": T234; "apple": T235; };
type T237 = { "recorder": T0; };
type T238 = { "recorder": T0; };
type T239 = { "recorder": T0; };
type T240 = { "interval": T139; "recorder": T0; };
type T241 = { "type": T0; };
type T242 = { "history": T139; "interval": T139; "track": T0; };
type T243 = { "slug": T0; "id": T0; };
type T244 = { "id": T13; "name": T0; "url": T0; "tab": T0; "params": T243; "fragment": T0; };
type T245 = T244[];
type T246 = { "name": T0; "stack": T245; };
type T247 = T246[];
type T248 = { "tab": T0; "tabs": T247; "next": T13; };
type T249 = null;
type T250 = T65 | null;
type T251 = T82 | null;
type T252 = T75 | null;
type T253 = T94 | null;
type T254 = T122 | null;
type T255 = T90 | null;
type T256 = T162 | null;
type T257 = T179 | null;
type T258 = T160 | null;
type T259 = T81 | null;
type T260 = { "key": T0; "shiftKey": T9; "ctrlKey": T9; "altKey": T9; "metaKey": T9; "code": T0; "repeat": T9; "isComposing": T9; };
type T261 = { "offsetX": T13; "offsetY": T13; "button": T13; "buttons": T13; "pressure": T13; "pointerType": T0; "pointerId": T13; "clientX": T13; "clientY": T13; "shiftKey": T9; "ctrlKey": T9; "altKey": T9; "metaKey": T9; };
type T262 = { "offsetX": T13; "offsetY": T13; "deltaX": T13; "deltaY": T13; "deltaMode": T13; "shiftKey": T9; "ctrlKey": T9; "altKey": T9; "metaKey": T9; };
type T263 = { "text": T0; "files": T16; };
type T264 = { "value": T0; "checked": T9; "selectionStart": T13; "selectionEnd": T13; "selectionDirection": T0; "inputType": T0; "data": T1; "cancelable": T9; "isComposing": T9; };
type T265 = { "path": T0; "name": T0; "type": T0; "size": T13; "width": T139; "height": T139; "duration": T139; };
type T266 = T265[];
type T267 = { "offsetX": T13; "offsetY": T13; "files": T16; "shiftKey": T9; "ctrlKey": T9; "altKey": T9; "metaKey": T9; };
type T268 = { "scrollLeft": T13; "scrollTop": T13; "scrollWidth": T13; "scrollHeight": T13; "clientWidth": T13; "clientHeight": T13; "firstVisibleKey": T1; "firstVisibleOffset": T13; };
type T269 = { "target": T0; "relatedTarget": T1; };

/** Sources called by compiled mutation sends. */
export type MutationSource = "checkAnytype" | "closeComputer" | "computerControl" | "computerInput" | "confirmPairing" | "copyStorage" | "createBot" | "createGroup" | "createManagementMcpCredential" | "createSubagent" | "deleteBot" | "deleteGroup" | "deleteLogin" | "dropVoice" | "enableNotifications" | "hideBot" | "makeMainBot" | "markRoomSeen" | "markSeen" | "mergeSide" | "noteUnread" | "openComputer" | "openSide" | "pinBot" | "pinMessage" | "prepareAnytype" | "reactTo" | "readHealth" | "recolourBot" | "renameBot" | "reshapeBot" | "respondRequest" | "restartGateway" | "restartStorage" | "revealPassword" | "revokeManagementMcpCredential" | "saveBotMemory" | "saveBotPrompt" | "saveDrafts" | "saveLogin" | "saveSpace" | "saveStorage" | "saveSubagent" | "sendMessage" | "sendRoom" | "sendVoice" | "setBotNotifications" | "setComputerPlace" | "setDefaultMode" | "setDefaultModel" | "setGroupAnswerPolicy" | "setManagementMcp" | "setModel" | "setNetwork" | "setNotifications" | "setPassword" | "setTheme" | "setUserName" | "signIn" | "signOut" | "stageVoice" | "startPairing" | "startUpdate" | "stopRun" | "testStorage" | "updateGroup" | "uploadAttachment";

export interface SourceMap {
  "checkAnytype": { args: []; result: T70 };
  "closeComputer": { args: [T0]; result: T63 };
  "computerControl": { args: [T0, T0, T0]; result: T63 };
  "computerInput": { args: [T0, T0, T13, T13, T13, T13, T13, T0, T13, T13]; result: T65 };
  "confirmPairing": { args: [T0, T0]; result: T63 };
  "copyStorage": { args: [T0]; result: T63 };
  "createBot": { args: [T0, T0, T0, T0, T0]; result: T76 };
  "createGroup": { args: [T0, T0, T0, T0, T0, T0, T0]; result: T76 };
  "createManagementMcpCredential": { args: [T0, T0]; result: T94 };
  "createSubagent": { args: [T0, T0, T0, T0, T0, T9]; result: T63 };
  "deleteBot": { args: [T0]; result: T63 };
  "deleteGroup": { args: [T0]; result: T63 };
  "deleteLogin": { args: [T0]; result: T63 };
  "dropVoice": { args: [T0, T0]; result: T63 };
  "enableNotifications": { args: []; result: T63 };
  "hideBot": { args: [T0, T9]; result: T63 };
  "loadAbout": { args: []; result: T72 };
  "loadBots": { args: [T0]; result: T96 };
  "loadChat": { args: [T0, T0, T0]; result: T26 };
  "loadComputer": { args: [T0, T0, T0, T0, T0]; result: T62 };
  "loadComputerPlace": { args: [T0, T0, T0]; result: T126 };
  "loadConvos": { args: [T0, T0, T0]; result: T97 };
  "loadDrafts": { args: []; result: T84 };
  "loadLineage": { args: [T0, T0, T0]; result: T49 };
  "loadManagementMcp": { args: [T0]; result: T93 };
  "loadMemory": { args: [T0, T0]; result: T121 };
  "loadModels": { args: [T0, T0, T0]; result: T43 };
  "loadNetwork": { args: [T0]; result: T67 };
  "loadNotice": { args: []; result: T79 };
  "loadPageOrigin": { args: []; result: T182 };
  "loadPrefs": { args: []; result: T77 };
  "loadProviders": { args: []; result: T42 };
  "loadRoom": { args: [T0]; result: T31 };
  "loadRoutines": { args: [T0, T0, T0]; result: T46 };
  "loadSession": { args: [T0]; result: T66 };
  "loadShelf": { args: [T0, T0]; result: T59 };
  "loadSideChats": { args: [T0]; result: T89 };
  "loadStorage": { args: [T0]; result: T68 };
  "loadSuggestions": { args: []; result: T105 };
  "loadSystem": { args: [T0]; result: T73 };
  "loadUsage": { args: [T0, T0]; result: T83 };
  "loadUsageReport": { args: [T0, T0, T0, T0]; result: T56 };
  "loadVault": { args: []; result: T130 };
  "makeMainBot": { args: [T0]; result: T63 };
  "markRoomSeen": { args: [T0]; result: T63 };
  "markSeen": { args: [T0]; result: T63 };
  "mergeSide": { args: [T0, T0]; result: T63 };
  "noteUnread": { args: [T0, T9, T9, T0, T9, T9, T9, T0, T0, T0, T0]; result: T63 };
  "openComputer": { args: [T0]; result: T63 };
  "openSide": { args: [T0, T0]; result: T90 };
  "pinBot": { args: [T0, T9]; result: T63 };
  "pinMessage": { args: [T0, T0, T0, T0]; result: T63 };
  "prepareAnytype": { args: []; result: T63 };
  "reactTo": { args: [T0, T0, T0, T0]; result: T63 };
  "readHealth": { args: []; result: T75 };
  "readPromptLink": { args: [T0]; result: T181 };
  "recolourBot": { args: [T0, T0]; result: T63 };
  "renameBot": { args: [T0, T0]; result: T63 };
  "reshapeBot": { args: [T0, T0]; result: T63 };
  "respondRequest": { args: [T0, T0, T0, T0, T0, T0, T0]; result: T63 };
  "restartGateway": { args: []; result: T63 };
  "restartStorage": { args: []; result: T63 };
  "revealPassword": { args: [T0]; result: T82 };
  "revokeManagementMcpCredential": { args: [T0]; result: T94 };
  "saveBotMemory": { args: [T0, T0, T0]; result: T122 };
  "saveBotPrompt": { args: [T0, T0]; result: T63 };
  "saveDrafts": { args: [T16, T16, T16]; result: T63 };
  "saveLogin": { args: [T0, T0, T0, T0, T0, T0]; result: T63 };
  "saveSpace": { args: [T0]; result: T63 };
  "saveStorage": { args: [T0, T0, T0]; result: T63 };
  "saveSubagent": { args: [T0, T0, T0, T0, T0, T9]; result: T63 };
  "sendMessage": { args: [T0, T0, T0, T0, T0, T0, T0, T0]; result: T64 };
  "sendRoom": { args: [T0, T0, T0]; result: T64 };
  "sendVoice": { args: [T0, T13, T0, T0, T0]; result: T63 };
  "setBotNotifications": { args: [T0, T9]; result: T63 };
  "setComputerPlace": { args: [T0, T0]; result: T63 };
  "setDefaultMode": { args: [T0]; result: T63 };
  "setDefaultModel": { args: [T0, T0, T0]; result: T63 };
  "setGroupAnswerPolicy": { args: [T0]; result: T63 };
  "setManagementMcp": { args: [T9]; result: T94 };
  "setModel": { args: [T0, T0, T0, T0, T0]; result: T63 };
  "setNetwork": { args: [T0]; result: T63 };
  "setNotifications": { args: [T0, T9, T0, T0]; result: T63 };
  "setPassword": { args: [T0]; result: T63 };
  "setTheme": { args: [T0]; result: T63 };
  "setUserName": { args: [T0]; result: T63 };
  "signIn": { args: [T0]; result: T63 };
  "signOut": { args: []; result: T63 };
  "stageVoice": { args: [T0, T0, T0, T13]; result: T81 };
  "startPairing": { args: []; result: T69 };
  "startUpdate": { args: []; result: T74 };
  "stopRun": { args: [T0, T0, T0]; result: T63 };
  "testStorage": { args: [T0]; result: T63 };
  "updateGroup": { args: [T0, T0, T0]; result: T63 };
  "uploadAttachment": { args: [T0, T0, T0, T0, T0]; result: T80 };
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
