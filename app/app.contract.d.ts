// @generated from Contract source signatures; do not edit.
// Static types do not replace runtime shape, identity, or grant checks.
//
// What app.ts writes, in brief (the declarations follow): `sources: Sources` has one
// function per source, given its arguments as an array (then store, storage, native) and
// answering its shape, or a Promise of it.
//   askDiagnostics: (args: [string]) => Reply
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
//   loadActivity: (args: [string, string, string, string]) => ActivityDetail
//   loadBots: (args: [string]) => Bot[]
//   loadChat: (args: [string, string, string]) => Chat
//   loadCode: (args: [string, string, string]) => CodeView
//   loadComputer: (args: [string, string, string, string, string]) => Computer
//   loadComputerPlace: (args: [string, string, string]) => ComputerPlace
//   loadConvos: (args: [string, string, string]) => Convo[]
//   loadDiagnostics: (args: [string]) => Diagnostics
//   loadDrafts: (args: []) => DraftPack
//   loadHelperPolicy: (args: [string, string, string]) => HelpPolicy
//   loadLimits: (args: [string, string]) => LimitList
//   loadLineage: (args: [string, string, string]) => Lineage
//   loadMachines: (args: [string, string]) => MachineList
//   loadManagementMcp: (args: [string]) => McpStatus
//   loadMemory: (args: [string, string]) => MemoryFile
//   loadModels: (args: [string, string, string]) => ModelList
//   loadNetwork: (args: [string]) => Network
//   loadNotice: (args: []) => NoticeState
//   loadPageOrigin: (args: []) => PageOrigin
//   loadPrefs: (args: [string]) => Prefs
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
//   refreshLimits: (args: []) => LimitReply
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
//   saveHelperPolicy: (args: [string, string, boolean, string, string, string, string, boolean, string, string, boolean, string, string, string]) => Reply
//   saveLogin: (args: [string, string, string, string, string, string]) => Reply
//   saveSections: (args: [string[]]) => Reply
//   saveSpace: (args: [string]) => Reply
//   saveStorage: (args: [string, string, string]) => Reply
//   saveSubagent: (args: [string, string, string, string, string, boolean]) => Reply
//   sendMessage: (args: [string, string, string, string, string, string, string, string]) => Sent
//   sendRoom: (args: [string, string, string]) => Sent
//   sendVoice: (args: [string, number, string, string, string]) => Reply
//   setActivityLevel: (args: [string]) => Reply
//   setBotNotifications: (args: [string, boolean]) => Reply
//   setComputerPlace: (args: [string, string]) => Reply
//   setDefaultMode: (args: [string]) => Reply
//   setDefaultModel: (args: [string, string, string]) => Reply
//   setFinalOnly: (args: [string]) => Reply
//   setGroupAnswerPolicy: (args: [string]) => Reply
//   setManagementMcp: (args: [boolean]) => McpResult
//   setMaxMode: (args: [string]) => Reply
//   setModel: (args: [string, string, string, string, string, string]) => Reply
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
// The shapes. None is exported: name one by its source, `type Reply = Result<'askDiagnostics'>`.
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
//   LimitWindow = { id: string; label: string; usedPercent: number; percent: string; resetLabel: string }
//   ProviderLimit = { instanceId: string; label: string; state: string; note: string; hint: string; windows: LimitWindow[] }
//   LimitList = { providers: ProviderLimit[]; sample: boolean; error: string; at: string }
//   LimitReply = { ok: boolean; error: string; cooling: boolean }
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
//   Section = { id: string; name: string; collapsed: boolean; sort: string; items: string[] }
//   ActivityDetail = { id: string; input: string; output: string; truncated: boolean; missing: boolean }
//   Prefs = { userName: string; theme: string; defaultInstance: string; defaultModel: string; defaultEffort: string; defaultRuntimeMode: string; noticeEnabled: boolean; noticeSound: boolean; noticePreview: string; noticeWhen: string; noticeEnabledMissing: boolean; groupAnswerPolicy: string; maxRuntimeMode: string; codeBeta: boolean; sections: Section[]; activityLevel: string; finalOnly: boolean }
//   Diagnostics = { ok: boolean; text: string; error: string }
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
//   Machine = { id: string; label: string; local: boolean; status: string; laptop: boolean; lastSeenAt: string; t3Version: string; lastError: string; access: string }
//   MachineList = { ok: boolean; machines: Machine[]; error: string }
//   HelpSide = { enabled: boolean; cap: string; when: string; mode: string; effort: string; inherit: boolean; modelInstance: string; modelSlug: string; anyModel: boolean; modelList: string; modes: string; efforts: string }
//   HelpPolicy = { ok: boolean; error: string; lasting: HelpSide; once: HelpSide }
//   MemoryFile = { ok: boolean; path: string; text: string; revision: string; truncated: boolean }
//   MemorySave = { ok: boolean; error: string; conflict: boolean; text: string; revision: string }
//   ComputerLink = { id: string; label: string; status: string; access: string; selectable: boolean }
//   ComputerPlace = { ok: boolean; selected: string; localLabel: string; links: ComputerLink[]; waiting: boolean; error: string }
//   CodeFile = { path: string; insertions: number; deletions: number }
//   CodePr = { number: number; title: string; url: string; draft: boolean; checks: string; review: string; updated: string }
//   CodeView = { enabled: boolean; signal: boolean; loaded: boolean; error: string; branch: string; defaultBranch: boolean; ahead: number; behind: number; hasRemote: boolean; provider: string; baseUrl: string; insertions: number; deletions: number; fileTotal: number; files: CodeFile[]; linkedNumber: number; linkedTitle: string; linkedUrl: string; prs: CodePr[] }
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
type T63 = { "id": T0; "label": T0; "usedPercent": T13; "percent": T0; "resetLabel": T0; };
type T64 = T63[];
type T65 = { "instanceId": T0; "label": T0; "state": T0; "note": T0; "hint": T0; "windows": T64; };
type T66 = T65[];
type T67 = { "providers": T66; "sample": T9; "error": T0; "at": T0; };
type T68 = { "ok": T9; "error": T0; "cooling": T9; };
type T69 = { "ok": T9; "error": T0; };
type T70 = { "ok": T9; "error": T0; "messageId": T0; };
type T71 = { "ok": T9; "error": T0; "text": T0; };
type T72 = { "authenticated": T9; "passwordRequired": T9; };
type T73 = { "mode": T0; "locked": T9; "restartRequired": T9; "canRestart": T9; "listening": T0; "urls": T0; "error": T0; "http": T9; };
type T74 = { "backend": T0; "root": T0; "backendLock": T0; "rootLock": T0; "restartRequired": T9; "canRestart": T9; "error": T0; "keySet": T9; "space": T0; "spaceLock": T0; };
type T75 = { "ok": T9; "error": T0; "challengeId": T0; };
type T76 = { "ok": T9; "error": T0; "space": T0; "spaceFound": T9; "shared": T9; "schemaReady": T9; };
type T77 = { "name": T0; "about": T0; "color": T0; "shape": T0; "first": T0; };
type T78 = { "ok": T9; "version": T0; "harness": T0; "connected": T9; };
type T79 = { "ok": T9; "installed": T9; "service": T9; "version": T0; "latest": T0; "updateAvailable": T9; "error": T0; };
type T80 = { "ok": T9; "version": T0; "error": T0; };
type T81 = { "ok": T9; "version": T0; };
type T82 = { "ok": T9; "id": T0; "error": T0; };
type T83 = { "id": T0; "name": T0; "collapsed": T9; "sort": T0; "items": T16; };
type T84 = T83[];
type T85 = { "userName": T0; "theme": T0; "defaultInstance": T0; "defaultModel": T0; "defaultEffort": T0; "defaultRuntimeMode": T0; "noticeEnabled": T9; "noticeSound": T9; "noticePreview": T0; "noticeWhen": T0; "noticeEnabledMissing": T9; "groupAnswerPolicy": T0; "maxRuntimeMode": T0; "codeBeta": T9; "sections": T84; "activityLevel": T0; "finalOnly": T9; };
type T86 = { "ok": T9; "text": T0; "error": T0; };
type T87 = { "visibilityState": T0; "hasFocus": T9; };
type T88 = { "available": T9; "state": T0; };
type T89 = { "ok": T9; "id": T0; "name": T0; "error": T0; };
type T90 = { "ok": T9; "id": T0; "name": T0; "url": T0; "seconds": T13; "error": T0; };
type T91 = { "ok": T9; "id": T0; "password": T0; "error": T0; };
type T92 = { "ok": T9; "error": T0; "messages": T13; "attachments": T13; "vaultWrites": T13; "iconFetches": T13; };
type T93 = { "stamp": T0; "texts": T16; "files": T16; "voices": T16; };
type T94 = { "locale": T0; };
type T95 = { "hover": T0; };
type T96 = { "id": T0; "title": T0; };
type T97 = T96[];
type T98 = { "chats": T97; };
type T99 = { "ok": T9; "error": T0; "threadId": T0; "messageId": T0; };
type T100 = { "id": T0; "label": T0; "permissions": T0; };
type T101 = T100[];
type T102 = { "loaded": T9; "enabled": T9; "endpoint": T0; "credentials": T101; };
type T103 = { "ok": T9; "error": T0; "token": T0; };
type T104 = { "id": T0; "label": T0; "local": T9; "status": T0; "laptop": T9; "lastSeenAt": T0; "t3Version": T0; "lastError": T0; "access": T0; };
type T105 = T104[];
type T106 = { "ok": T9; "machines": T105; "error": T0; };
type T107 = T76 | null;
type T108 = T12[];
type T109 = T35[];
type T110 = T69 | null;
type T111 = T11 | null;
type T112 = T75 | null;
type T113 = T80 | null;
type T114 = { "mcpStatus": T102; "mcpToken": T0; "mcpError": T0; "mcpPending": T9; "about": T78; "anytypeChecked": T107; "anytypePrepareError": T0; "anytypePreparedPending": T9; "anytypeError": T0; "anytypeFound": T9; "anytypeOn": T9; "anytypeShared": T9; "anytypeSpace": T0; "bots": T108; "canUpdate": T9; "convos": T109; "nameDraft": T0; "nameEditing": T9; "network": T73; "networkError": T0; "networkRestartError": T0; "networkRestarted": T110; "networkSet": T110; "newPassword": T0; "notice": T88; "noticeAsked": T111; "pairCode": T0; "pairConfirmError": T0; "pairConfirmed": T110; "pairError": T0; "pairStarted": T112; "pairStep": T0; "passwordError": T0; "passwordSet": T110; "prefs": T85; "prefsError": T0; "providers": T42; "limits": T67; "limitCooling": T9; "limitError": T0; "report": T56; "reportFailed": T9; "session": T72; "settingsDomain": T0; "shownBackend": T0; "spaceEditing": T9; "spaceName": T0; "spaceSaveError": T0; "spaceSaved": T110; "storage": T74; "storageCopied": T110; "storageCopyError": T0; "storageHint": T0; "storagePath": T0; "storageRestartError": T0; "storageRestarted": T110; "storageSaveError": T0; "storageSaved": T110; "storageTestError": T0; "storageTested": T110; "system": T79; "updateError": T0; "updateFrom": T0; "updateNeedsService": T9; "updateStarted": T113; "updateStuck": T9; "updateTo": T0; "usagePeriod": T0; "usageView": T0; "anytypeCheckedPending": T9; "networkRestartedPending": T9; "networkSetPending": T9; "noticeAskedPending": T9; "pairConfirmedPending": T9; "pairStartedPending": T9; "passwordSetPending": T9; "spaceSavedPending": T9; "storageCopiedPending": T9; "storageRestartedPending": T9; "storageSavedPending": T9; "storageTestedPending": T9; "systemPending": T9; "updateStartedPending": T9; "machines": T106; "machinesFailed": T9; "machinesPending": T9; "diagnostics": T86; "diagAskError": T0; "diagAskPending": T9; };
type T115 = T82 | null;
type T116 = { "bots": T108; "canMakeGroup": T9; "groupAbout": T0; "groupChief": T0; "groupError": T0; "groupMade": T115; "groupMembers": T16; "groupName": T0; "groupPolicy": T0; "groupStep": T0; "pickableBots": T108; "groupMadePending": T9; };
type T117 = T77[];
type T118 = { "createError": T0; "createStep": T0; "createdPending": T9; "newColor": T0; "newFirst": T0; "newName": T0; "newPrompt": T0; "newShape": T0; "suggestions": T117; };
type T119 = { "accessibilityTextSize": T9; };
type T120 = { "prefersContrast": T0; };
type T121 = { "key": T0; "kind": T0; "text": T0; "slug": T0; "name": T0; "color": T0; "shape": T0; };
type T122 = { "key": T0; "text": T0; "start": T13; "n": T13; };
type T123 = { "start": T13; "query": T0; };
type T124 = { "text": T0; "cursor": T13; };
type T125 = { "start": T13; "end": T13; "slug": T0; "kind": T0; "name": T0; "color": T0; "shape": T0; };
type T126 = T32[];
type T127 = T70 | null;
type T128 = { "answerError": T0; "answerText": T0; "answered": T110; "answeredPending": T9; "away": T9; "bots": T108; "chat": T26; "copiedMenu": T0; "groupPage": T0; "groupPanel": T9; "groupSaveError": T0; "mentionHits": T126; "mentionIdx": T13; "messageMenu": T0; "paintedWorkers": T29; "pointerFacts": T95; "prefs": T85; "reactFor": T0; "room": T31; "roomError": T0; "roomPending": T0; "roomRows": T24; "roomSent": T127; "roomText": T0; "wide": T9; "roomSentPending": T9; "roomFailed": T9; };
type T129 = { "toast": T0; };
type T130 = { "computerEnlarged": T9; "computer": T62; "onComputer": T9; "holding": T9; "locked": T9; "bots": T108; "slug": T0; };
type T131 = { "system": T79; "password": T0; "signedIn": T110; "signInError": T0; "locked": T9; "signedInPending": T9; };
type T132 = { "rows": T24; "working": T9; "draft": T0; "canSend": T9; "pending": T16; "attachError": T0; "attachedPending": T9; "sentPending": T9; "replyTo": T0; "replyQuote": T0; "voicePreview": T0; "voiceError": T0; "voiceHold": T0; "voiceId": T0; "recording": T9; "voiceSentPending": T9; "answerText": T0; "answerError": T0; "answering": T9; "canMerge": T9; "name": T0; "bots": T108; "slug": T0; "pins": T25; "hover": T9; "reactFor": T0; "messageMenu": T0; "copiedMenu": T0; "empty": T9; "failed": T9; "loading": T9; };
type T133 = { "enabled": T9; "cap": T0; "when": T0; "mode": T0; "effort": T0; "inherit": T9; "modelInstance": T0; "modelSlug": T0; "anyModel": T9; "modelList": T0; "modes": T0; "efforts": T0; };
type T134 = { "ok": T9; "error": T0; "lasting": T133; "once": T133; };
type T135 = { "policy": T134; "failed": T9; "pending": T9; "saving": T9; "error": T0; "seeded": T9; "whenLasting": T0; "whenOnce": T0; "capLasting": T0; "capOnce": T0; };
type T136 = { "ok": T9; "path": T0; "text": T0; "revision": T0; "truncated": T9; };
type T137 = { "ok": T9; "error": T0; "conflict": T9; "text": T0; "revision": T0; };
type T138 = { "file": T136; "failed": T9; "pending": T9; "editing": T9; "draft": T0; "conflict": T9; "serverText": T0; "serverRevision": T0; "error": T0; "saving": T9; };
type T139 = { "id": T0; "label": T0; "status": T0; "access": T0; "selectable": T9; };
type T140 = T139[];
type T141 = { "ok": T9; "selected": T0; "localLabel": T0; "links": T140; "waiting": T9; "error": T0; };
type T142 = { "path": T0; "insertions": T13; "deletions": T13; };
type T143 = { "number": T13; "title": T0; "url": T0; "draft": T9; "checks": T0; "review": T0; "updated": T0; };
type T144 = T142[];
type T145 = T143[];
type T146 = { "enabled": T9; "signal": T9; "loaded": T9; "error": T0; "branch": T0; "defaultBranch": T9; "ahead": T13; "behind": T13; "hasRemote": T9; "provider": T0; "baseUrl": T0; "insertions": T13; "deletions": T13; "fileTotal": T13; "files": T144; "linkedNumber": T13; "linkedTitle": T0; "linkedUrl": T0; "prs": T145; };
type T147 = T89 | null;
type T148 = { "system": T79; "models": T43; "modelError": T0; "shownRows": T24; "searchNote": T0; "away": T9; "answerText": T0; "chatSearching": T9; "messageQuery": T0; "reactFor": T0; "messageMenu": T0; "copiedMenu": T0; "replyTo": T0; "replyQuote": T0; "childThread": T0; "sent": T127; "answered": T110; "attached": T147; "pendingText": T0; "attachError": T0; "answerError": T0; "paintedWorkers": T29; "draft": T0; "mentionIdx": T13; "mentionHits": T126; "usagePeriod": T0; "usageView": T0; "report": T56; "childModels": T43; "routines": T46; "lineage": T49; "shelf": T59; "voiceRecorder": T0; "voiceError": T0; "voiceSent": T110; "voiceHold": T0; "voiceId": T0; "voicePreview": T0; "wide": T9; "panelOpen": T9; "panelTab": T0; "panelPage": T0; "subSlug": T0; "subName": T0; "subShape": T0; "subColor": T0; "subPrompt": T0; "subInherit": T9; "subCreated": T110; "subSaved": T110; "botError": T0; "subError": T0; "subSaveError": T0; "childRemoveError": T0; "heldTab": T0; "computerTab": T0; "computer": T62; "controlError": T0; "openError": T0; "openingBrowser": T9; "closeError": T0; "closingBrowser": T9; "driveError": T0; "holding": T9; "pointerFacts": T95; "bots": T108; "page": T87; "chat": T26; "slug": T0; "pendingAttaches": T16; "canSend": T9; "chatFailed": T9; "chatPending": T9; "answeredPending": T9; "attachedPending": T9; "voiceSentPending": T9; "sentPending": T9; "modelsFailed": T9; "routinesFailed": T9; "lineageFailed": T9; "reportFailed": T9; "shelfFailed": T9; "computerFailed": T9; "subCreatedPending": T9; "subSavedPending": T9; "childModelsFailed": T9; "linkDraft": T9; "linkLocal": T9; "limits": T66; "limitFocus": T0; "switchBot": T0; "switchFrom": T0; "switchTo": T0; "modelSavedPending": T9; "codeOn": T9; "code": T146; };
type T149 = { "prefs": T85; "botQuery": T0; "searching": T9; "hiddenOpen": T9; "rowMenu": T0; "firstRun": T9; "visibleBots": T108; "tiles": T108; "hiddenBots": T108; "newColor": T0; "newShape": T0; "wide": T9; "plusOpen": T9; "bots": T108; "convos": T109; "sectionHomes": T16; "chat": T26; "slug": T0; "query": T0; "roomId": T0; "convoName": T0; "botsFailed": T9; };
type T150 = T36[];
type T151 = { "vaultQuery": T0; "vault": T150; "vquery": T0; "shownVault": T150; "query": T0; "vaultFailed": T9; "vaultPending": T9; };
type T152 = { "permanentlyDenied": T9; "shouldShowRationale": T9; };
type T153 = T152 | null;
type T154 = "notDetermined" | "restricted" | "denied" | "authorized";
type T155 = { "authorizationStatus": T154; };
type T156 = T155 | null;
type T157 = "granted" | "prompt" | "denied" | "unknown";
type T158 = { "android": T153; "apple": T156; "available": T9; "state": T157; };
type T159 = T13 | null;
type T160 = { "max": T13; "min": T13; "step": T159; };
type T161 = { "refused": T1; };
type T162 = "audioinput" | "videoinput" | "audiooutput";
type T163 = { "deviceId": T0; "facingMode": T1; "groupId": T0; "kind": T162; "label": T0; };
type T164 = T163[];
type T165 = { "devices": T164; };
type T166 = { "cameraId": T1; "isClientSilenced": T9; "lensFacing": T1; };
type T167 = T166 | null;
type T168 = { "deviceType": T1; "portName": T1; "portType": T1; "position": T1; };
type T169 = T168 | null;
type T170 = T160 | null;
type T171 = T16 | null;
type T172 = T9 | null;
type T173 = { "aspectRatio": T170; "facingMode": T171; "frameRate": T170; "height": T170; "torch": T172; "width": T170; "zoom": T170; };
type T174 = T173 | null;
type T175 = "audio" | "video";
type T176 = "idle" | "interrupted" | "background" | "silenced" | "in-use" | "multitasking" | "system-pressure";
type T177 = T176 | null;
type T178 = "live" | "ended";
type T179 = { "aspectRatio": T159; "autoGainControl": T172; "channelCount": T159; "deviceId": T1; "echoCancellation": T172; "facingMode": T1; "frameRate": T159; "groupId": T1; "height": T159; "latency": T159; "noiseSuppression": T172; "resizeMode": T1; "sampleRate": T159; "sampleSize": T159; "torch": T172; "width": T159; "zoom": T159; };
type T180 = { "android": T167; "apple": T169; "capabilities": T174; "enabled": T9; "id": T0; "kind": T175; "label": T0; "muteReason": T177; "muted": T9; "readyState": T178; "settings": T179; };
type T181 = T180[];
type T182 = { "active": T9; "id": T0; "tracks": T181; };
type T183 = { "aspectRatio": T159; "autoGainControl": T172; "channelCount": T159; "deviceId": T1; "echoCancellation": T172; "facingMode": T1; "frameRate": T159; "groupId": T1; "height": T159; "latency": T159; "noiseSuppression": T172; "resizeMode": T1; "sampleRate": T159; "sampleSize": T159; "torch": T172; "width": T159; "zoom": T159; };
type T184 = { "permanentlyDenied": T9; "shouldShowRationale": T9; };
type T185 = T184 | null;
type T186 = "undetermined" | "denied" | "granted" | "notDetermined" | "restricted" | "authorized";
type T187 = { "recordPermission": T186; };
type T188 = T187 | null;
type T189 = "granted" | "prompt" | "denied" | "unknown";
type T190 = { "android": T185; "apple": T188; "available": T9; "state": T189; };
type T191 = { "x": T13; "y": T13; };
type T192 = T13[];
type T193 = { "audioLevel": T13; "levels": T192; "totalAudioEnergy": T13; "totalSamplesDuration": T13; };
type T194 = "requested" | "interrupted" | "media-reset" | "background" | "track-ended" | "error" | "session-ended";
type T195 = T194 | null;
type T196 = { "duration": T13; "mimeType": T0; "path": T0; "size": T13; };
type T197 = T196 | null;
type T198 = "inactive" | "recording" | "paused";
type T199 = { "audioBitsPerSecond": T159; "duration": T13; "error": T1; "id": T0; "mimeType": T0; "path": T0; "reason": T195; "result": T197; "state": T198; };
type T200 = { "supported": T9; "type": T0; };
type T201 = { "bot": T0; "prompt": T0; "present": T9; "bad": T9; };
type T202 = { "origin": T0; "local": T9; };
type T203 = "passive" | "active" | "timeSensitive";
type T204 = T203 | null;
type T205 = { "interruptionLevel": T204; "relevanceScore": T159; "subtitle": T1; "targetContentIdentifier": T1; "threadIdentifier": T1; };
type T206 = T205 | null;
type T207 = { "body": T1; "data": T1; "navigate": T1; "showTrigger": T159; "silent": T172; "tag": T1; "title": T0; "apple": T206; };
type T208 = { "tag": T0; };
type T209 = { "contents": T159; };
type T210 = "mic" | "camcorder" | "voiceRecognition" | "voiceCommunication" | "unprocessed" | "voicePerformance";
type T211 = T210 | null;
type T212 = { "audioSource": T211; "privacySensitive": T172; };
type T213 = "mixWithOthers" | "duckOthers" | "allowBluetoothHFP" | "allowBluetoothA2DP" | "defaultToSpeaker" | "bluetoothHighQualityRecording" | "farFieldInput" | "overrideMutedMicrophoneInterruption";
type T214 = T213[];
type T215 = T214 | null;
type T216 = "builtInWideAngleCamera" | "builtInUltraWideCamera" | "builtInTelephotoCamera" | "builtInDualCamera" | "builtInDualWideCamera" | "builtInTripleCamera" | "builtInTrueDepthCamera" | "builtInLiDARDepthCamera" | "continuityCamera" | "deskViewCamera" | "external";
type T217 = T216 | null;
type T218 = "default" | "measurement" | "voiceChat" | "videoChat" | "videoRecording";
type T219 = T218 | null;
type T220 = "stereo" | "cardioid" | "omnidirectional";
type T221 = T220 | null;
type T222 = { "categoryOptions": T215; "deviceType": T217; "inputDataSource": T1; "mode": T219; "polarPattern": T221; "preferredIOBufferDuration": T159; "prefersNoInterruptionsFromSystemAlerts": T172; };
type T223 = T183 | null;
type T224 = T212 | null;
type T225 = T222 | null;
type T226 = { "audio": T172; "audioConstraints": T223; "video": T172; "videoConstraints": T223; "android": T224; "apple": T225; };
type T227 = "builtInWideAngleCamera" | "builtInUltraWideCamera" | "builtInTelephotoCamera" | "builtInDualCamera" | "builtInDualWideCamera" | "builtInTripleCamera" | "builtInTrueDepthCamera" | "builtInLiDARDepthCamera" | "continuityCamera" | "deskViewCamera" | "external";
type T228 = T227 | null;
type T229 = "locked" | "autoExpose" | "continuousAutoExposure";
type T230 = T229 | null;
type T231 = T191 | null;
type T232 = "locked" | "autoFocus" | "continuousAutoFocus";
type T233 = T232 | null;
type T234 = { "deviceType": T228; "exposureMode": T230; "exposurePointOfInterest": T231; "focusMode": T233; "focusPointOfInterest": T231; };
type T235 = T234 | null;
type T236 = { "aspectRatio": T159; "facingMode": T1; "frameRate": T159; "height": T159; "resizeMode": T1; "torch": T172; "track": T0; "width": T159; "zoom": T159; "apple": T235; };
type T237 = { "track": T0; };
type T238 = { "enabled": T9; "track": T0; };
type T239 = { "track": T0; };
type T240 = "aac" | "heAac" | "opus";
type T241 = T240 | null;
type T242 = "mpeg4" | "ogg" | "webm";
type T243 = T242 | null;
type T244 = { "audioEncoder": T241; "outputFormat": T243; };
type T245 = "min" | "low" | "medium" | "high" | "max";
type T246 = T245 | null;
type T247 = "constant" | "longTermAverage" | "variableConstrained" | "variable";
type T248 = T247 | null;
type T249 = "aac" | "alac" | "linearPCM" | "opus";
type T250 = T249 | null;
type T251 = { "audioQuality": T246; "bitRateStrategy": T248; "formatID": T250; };
type T252 = "constant" | "variable";
type T253 = T252 | null;
type T254 = T244 | null;
type T255 = T251 | null;
type T256 = { "audioBitrateMode": T253; "audioBitsPerSecond": T159; "mimeType": T1; "path": T0; "stream": T0; "android": T254; "apple": T255; };
type T257 = { "recorder": T0; };
type T258 = { "recorder": T0; };
type T259 = { "recorder": T0; };
type T260 = { "interval": T159; "recorder": T0; };
type T261 = { "type": T0; };
type T262 = { "history": T159; "interval": T159; "track": T0; };
type T263 = { "slug": T0; "id": T0; };
type T264 = { "id": T13; "name": T0; "url": T0; "tab": T0; "params": T263; "fragment": T0; };
type T265 = T264[];
type T266 = { "name": T0; "stack": T265; };
type T267 = T266[];
type T268 = { "tab": T0; "tabs": T267; "next": T13; };
type T269 = null;
type T270 = T68 | null;
type T271 = T71 | null;
type T272 = T91 | null;
type T273 = T81 | null;
type T274 = T103 | null;
type T275 = T137 | null;
type T276 = T99 | null;
type T277 = T182 | null;
type T278 = T199 | null;
type T279 = T180 | null;
type T280 = T90 | null;
type T281 = { "key": T0; "shiftKey": T9; "ctrlKey": T9; "altKey": T9; "metaKey": T9; "code": T0; "repeat": T9; "isComposing": T9; };
type T282 = { "offsetX": T13; "offsetY": T13; "button": T13; "buttons": T13; "pressure": T13; "pointerType": T0; "pointerId": T13; "clientX": T13; "clientY": T13; "shiftKey": T9; "ctrlKey": T9; "altKey": T9; "metaKey": T9; };
type T283 = { "offsetX": T13; "offsetY": T13; "deltaX": T13; "deltaY": T13; "deltaMode": T13; "shiftKey": T9; "ctrlKey": T9; "altKey": T9; "metaKey": T9; };
type T284 = { "text": T0; "files": T16; };
type T285 = { "value": T0; "checked": T9; "selectionStart": T13; "selectionEnd": T13; "selectionDirection": T0; "inputType": T0; "data": T1; "cancelable": T9; "isComposing": T9; };
type T286 = { "path": T0; "name": T0; "type": T0; "size": T13; "width": T159; "height": T159; "duration": T159; };
type T287 = T286[];
type T288 = { "offsetX": T13; "offsetY": T13; "files": T16; "shiftKey": T9; "ctrlKey": T9; "altKey": T9; "metaKey": T9; };
type T289 = { "scrollLeft": T13; "scrollTop": T13; "scrollWidth": T13; "scrollHeight": T13; "clientWidth": T13; "clientHeight": T13; "firstVisibleKey": T1; "firstVisibleOffset": T13; };
type T290 = { "target": T0; "relatedTarget": T1; };
type T291 = { "id": T0; "input": T0; "output": T0; "truncated": T9; "missing": T9; };

/** Sources called by compiled mutation sends. */
export type MutationSource = "askDiagnostics" | "checkAnytype" | "closeComputer" | "computerControl" | "computerInput" | "confirmPairing" | "copyStorage" | "createBot" | "createGroup" | "createManagementMcpCredential" | "createSubagent" | "deleteBot" | "deleteGroup" | "deleteLogin" | "dropVoice" | "enableNotifications" | "hideBot" | "makeMainBot" | "markRoomSeen" | "markSeen" | "mergeSide" | "noteUnread" | "openComputer" | "openSide" | "pinBot" | "pinMessage" | "prepareAnytype" | "reactTo" | "readHealth" | "recolourBot" | "refreshLimits" | "renameBot" | "reshapeBot" | "respondRequest" | "restartGateway" | "restartStorage" | "revealPassword" | "revokeManagementMcpCredential" | "saveBotMemory" | "saveBotPrompt" | "saveDrafts" | "saveHelperPolicy" | "saveLogin" | "saveSections" | "saveSpace" | "saveStorage" | "saveSubagent" | "sendMessage" | "sendRoom" | "sendVoice" | "setActivityLevel" | "setBotNotifications" | "setComputerPlace" | "setDefaultMode" | "setDefaultModel" | "setFinalOnly" | "setGroupAnswerPolicy" | "setManagementMcp" | "setMaxMode" | "setModel" | "setNetwork" | "setNotifications" | "setPassword" | "setTheme" | "setUserName" | "signIn" | "signOut" | "stageVoice" | "startPairing" | "startUpdate" | "stopRun" | "testStorage" | "updateGroup" | "uploadAttachment";

export interface SourceMap {
  "askDiagnostics": { args: [T0]; result: T69 };
  "checkAnytype": { args: []; result: T76 };
  "closeComputer": { args: [T0]; result: T69 };
  "computerControl": { args: [T0, T0, T0]; result: T69 };
  "computerInput": { args: [T0, T0, T13, T13, T13, T13, T13, T0, T13, T13]; result: T71 };
  "confirmPairing": { args: [T0, T0]; result: T69 };
  "copyStorage": { args: [T0]; result: T69 };
  "createBot": { args: [T0, T0, T0, T0, T0]; result: T82 };
  "createGroup": { args: [T0, T0, T0, T0, T0, T0, T0]; result: T82 };
  "createManagementMcpCredential": { args: [T0, T0]; result: T103 };
  "createSubagent": { args: [T0, T0, T0, T0, T0, T9]; result: T69 };
  "deleteBot": { args: [T0]; result: T69 };
  "deleteGroup": { args: [T0]; result: T69 };
  "deleteLogin": { args: [T0]; result: T69 };
  "dropVoice": { args: [T0, T0]; result: T69 };
  "enableNotifications": { args: []; result: T69 };
  "hideBot": { args: [T0, T9]; result: T69 };
  "loadAbout": { args: []; result: T78 };
  "loadActivity": { args: [T0, T0, T0, T0]; result: T291 };
  "loadBots": { args: [T0]; result: T108 };
  "loadChat": { args: [T0, T0, T0]; result: T26 };
  "loadCode": { args: [T0, T0, T0]; result: T146 };
  "loadComputer": { args: [T0, T0, T0, T0, T0]; result: T62 };
  "loadComputerPlace": { args: [T0, T0, T0]; result: T141 };
  "loadConvos": { args: [T0, T0, T0]; result: T109 };
  "loadDiagnostics": { args: [T0]; result: T86 };
  "loadDrafts": { args: []; result: T93 };
  "loadHelperPolicy": { args: [T0, T0, T0]; result: T134 };
  "loadLimits": { args: [T0, T0]; result: T67 };
  "loadLineage": { args: [T0, T0, T0]; result: T49 };
  "loadMachines": { args: [T0, T0]; result: T106 };
  "loadManagementMcp": { args: [T0]; result: T102 };
  "loadMemory": { args: [T0, T0]; result: T136 };
  "loadModels": { args: [T0, T0, T0]; result: T43 };
  "loadNetwork": { args: [T0]; result: T73 };
  "loadNotice": { args: []; result: T88 };
  "loadPageOrigin": { args: []; result: T202 };
  "loadPrefs": { args: [T0]; result: T85 };
  "loadProviders": { args: []; result: T42 };
  "loadRoom": { args: [T0]; result: T31 };
  "loadRoutines": { args: [T0, T0, T0]; result: T46 };
  "loadSession": { args: [T0]; result: T72 };
  "loadShelf": { args: [T0, T0]; result: T59 };
  "loadSideChats": { args: [T0]; result: T98 };
  "loadStorage": { args: [T0]; result: T74 };
  "loadSuggestions": { args: []; result: T117 };
  "loadSystem": { args: [T0]; result: T79 };
  "loadUsage": { args: [T0, T0]; result: T92 };
  "loadUsageReport": { args: [T0, T0, T0, T0]; result: T56 };
  "loadVault": { args: []; result: T150 };
  "makeMainBot": { args: [T0]; result: T69 };
  "markRoomSeen": { args: [T0]; result: T69 };
  "markSeen": { args: [T0]; result: T69 };
  "mergeSide": { args: [T0, T0]; result: T69 };
  "noteUnread": { args: [T0, T9, T9, T0, T9, T9, T9, T0, T0, T0, T0]; result: T69 };
  "openComputer": { args: [T0]; result: T69 };
  "openSide": { args: [T0, T0]; result: T99 };
  "pinBot": { args: [T0, T9]; result: T69 };
  "pinMessage": { args: [T0, T0, T0, T0]; result: T69 };
  "prepareAnytype": { args: []; result: T69 };
  "reactTo": { args: [T0, T0, T0, T0]; result: T69 };
  "readHealth": { args: []; result: T81 };
  "readPromptLink": { args: [T0]; result: T201 };
  "recolourBot": { args: [T0, T0]; result: T69 };
  "refreshLimits": { args: []; result: T68 };
  "renameBot": { args: [T0, T0]; result: T69 };
  "reshapeBot": { args: [T0, T0]; result: T69 };
  "respondRequest": { args: [T0, T0, T0, T0, T0, T0, T0]; result: T69 };
  "restartGateway": { args: []; result: T69 };
  "restartStorage": { args: []; result: T69 };
  "revealPassword": { args: [T0]; result: T91 };
  "revokeManagementMcpCredential": { args: [T0]; result: T103 };
  "saveBotMemory": { args: [T0, T0, T0]; result: T137 };
  "saveBotPrompt": { args: [T0, T0]; result: T69 };
  "saveDrafts": { args: [T16, T16, T16]; result: T69 };
  "saveHelperPolicy": { args: [T0, T0, T9, T0, T0, T0, T0, T9, T0, T0, T9, T0, T0, T0]; result: T69 };
  "saveLogin": { args: [T0, T0, T0, T0, T0, T0]; result: T69 };
  "saveSections": { args: [T16]; result: T69 };
  "saveSpace": { args: [T0]; result: T69 };
  "saveStorage": { args: [T0, T0, T0]; result: T69 };
  "saveSubagent": { args: [T0, T0, T0, T0, T0, T9]; result: T69 };
  "sendMessage": { args: [T0, T0, T0, T0, T0, T0, T0, T0]; result: T70 };
  "sendRoom": { args: [T0, T0, T0]; result: T70 };
  "sendVoice": { args: [T0, T13, T0, T0, T0]; result: T69 };
  "setActivityLevel": { args: [T0]; result: T69 };
  "setBotNotifications": { args: [T0, T9]; result: T69 };
  "setComputerPlace": { args: [T0, T0]; result: T69 };
  "setDefaultMode": { args: [T0]; result: T69 };
  "setDefaultModel": { args: [T0, T0, T0]; result: T69 };
  "setFinalOnly": { args: [T0]; result: T69 };
  "setGroupAnswerPolicy": { args: [T0]; result: T69 };
  "setManagementMcp": { args: [T9]; result: T103 };
  "setMaxMode": { args: [T0]; result: T69 };
  "setModel": { args: [T0, T0, T0, T0, T0, T0]; result: T69 };
  "setNetwork": { args: [T0]; result: T69 };
  "setNotifications": { args: [T0, T9, T0, T0]; result: T69 };
  "setPassword": { args: [T0]; result: T69 };
  "setTheme": { args: [T0]; result: T69 };
  "setUserName": { args: [T0]; result: T69 };
  "signIn": { args: [T0]; result: T69 };
  "signOut": { args: []; result: T69 };
  "stageVoice": { args: [T0, T0, T0, T13]; result: T90 };
  "startPairing": { args: []; result: T75 };
  "startUpdate": { args: []; result: T80 };
  "stopRun": { args: [T0, T0, T0]; result: T69 };
  "testStorage": { args: [T0]; result: T69 };
  "updateGroup": { args: [T0, T0, T0]; result: T69 };
  "uploadAttachment": { args: [T0, T0, T0, T0, T0]; result: T89 };
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
