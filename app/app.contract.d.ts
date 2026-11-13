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
//   importSync: (args: [boolean]) => SyncImport
//   loadAbout: (args: []) => About
//   loadActivity: (args: [string, string, string, string]) => ActivityDetail
//   loadBots: (args: [string]) => Bot[]
//   loadChat: (args: [string, string, string]) => Chat
//   loadCode: (args: [string, string, string]) => CodeView
//   loadComputer: (args: [string, string, string, string, string]) => Computer
//   loadComputerPlace: (args: [string, string, string]) => ComputerPlace
//   loadConvos: (args: [string, string, string]) => Convo[]
//   loadDeliveries: (args: [string, string, string, string]) => DeliveryList
//   loadDelivery: (args: [string, string, string, string, string]) => DeliveryDetail
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
//   loadSync: (args: [string, string]) => SyncStatus
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
//   readHook: (args: [string, string, string]) => HookView
//   readPromptLink: (args: [string]) => PromptLink
//   recolourBot: (args: [string, string]) => Reply
//   refreshLimits: (args: []) => LimitReply
//   renameBot: (args: [string, string]) => Reply
//   reshapeBot: (args: [string, string]) => Reply
//   resolveConflict: (args: [string, string]) => Reply
//   respondRequest: (args: [string, string, string, string, string, string, string]) => Reply
//   restartGateway: (args: []) => Reply
//   restartStorage: (args: []) => Reply
//   revealPassword: (args: [string]) => Revealed
//   revokeManagementMcpCredential: (args: [string]) => McpResult
//   rotateHook: (args: [string, string, string]) => HookView
//   runSyncNow: (args: []) => Reply
//   saveBotMemory: (args: [string, string, string]) => MemorySave
//   saveBotPrompt: (args: [string, string]) => Reply
//   saveDrafts: (args: [string[], string[], string[]]) => Reply
//   saveHelperPolicy: (args: [string, string, boolean, string, string, string, string, boolean, string, string, boolean, string, string, string]) => Reply
//   saveLogin: (args: [string, string, string, string, string, string]) => Reply
//   saveRoutine: (args: [string, string, string]) => RoutineAddMade
//   saveSections: (args: [string[]]) => Reply
//   saveSpace: (args: [string]) => Reply
//   saveStorage: (args: [string, string, string]) => Reply
//   saveSubagent: (args: [string, string, string, string, string, boolean]) => Reply
//   saveWebhookRoutine: (args: [string, string, string, string, string, string, string, string, string, string, string, string, string, boolean]) => RoutineAddMade
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
//   testWebhookRoutine: (args: [string, string, string, string]) => RoutineTest
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
//   Routine = { id: string; title: string; paused: boolean; schedule: string; next: string; failed: boolean; kind: string; outcome: string }
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
//   Section = { id: string; name: string; collapsed: boolean; sort: string; items: string[] }
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
//   LimitWindow = { id: string; label: string; usedPercent: number; percent: string; resetLabel: string }
//   ProviderLimit = { instanceId: string; label: string; state: string; note: string; hint: string; windows: LimitWindow[] }
//   LimitList = { providers: ProviderLimit[]; sample: boolean; error: string; at: string }
//   LimitReply = { ok: boolean; error: string; cooling: boolean }
//   McpCredential = { id: string; label: string; permissions: string }
//   McpStatus = { loaded: boolean; enabled: boolean; endpoint: string; credentials: McpCredential[] }
//   McpResult = { ok: boolean; error: string; token: string }
//   Machine = { id: string; label: string; local: boolean; status: string; laptop: boolean; lastSeenAt: string; t3Version: string; lastError: string; access: string }
//   MachineList = { ok: boolean; machines: Machine[]; error: string }
//   SyncDevice = { name: string; lastSeen: string }
//   SyncConflict = { id: string; path: string }
//   SyncStatus = { ok: boolean; state: string; lastSync: string; pending: number; devices: SyncDevice[]; conflicts: SyncConflict[]; needsImport: boolean; error: string }
//   SyncImport = { ok: boolean; started: boolean; push: number; pull: number; conflict: number; minutes: number; error: string }
//   ActivityDetail = { id: string; input: string; output: string; truncated: boolean; missing: boolean }
//   HelpSide = { enabled: boolean; cap: string; when: string; mode: string; effort: string; inherit: boolean; modelInstance: string; modelSlug: string; anyModel: boolean; modelList: string; modes: string; efforts: string }
//   HelpPolicy = { ok: boolean; error: string; lasting: HelpSide; once: HelpSide }
//   MemoryFile = { ok: boolean; path: string; text: string; revision: string; truncated: boolean }
//   MemorySave = { ok: boolean; error: string; conflict: boolean; text: string; revision: string }
//   ComputerLink = { id: string; label: string; status: string; access: string; selectable: boolean }
//   ComputerPlace = { ok: boolean; selected: string; localLabel: string; links: ComputerLink[]; waiting: boolean; error: string }
//   CodeFile = { path: string; insertions: number; deletions: number }
//   CodePr = { number: number; title: string; url: string; draft: boolean; checks: string; review: string; updated: string }
//   CodeView = { enabled: boolean; signal: boolean; loaded: boolean; error: string; branch: string; defaultBranch: boolean; ahead: number; behind: number; hasRemote: boolean; provider: string; baseUrl: string; insertions: number; deletions: number; fileTotal: number; files: CodeFile[]; linkedNumber: number; linkedTitle: string; linkedUrl: string; prs: CodePr[] }
//   RoutineAddMade = { ok: boolean; error: string; id: string; url: string; source: string; stable: boolean }
//   RoutineTest = { ok: boolean; error: string; outcome: string }
//   DeliveryRow = { id: string; at: string; method: string; bytes: string; outcome: string }
//   DeliveryList = { ok: boolean; error: string; rows: DeliveryRow[] }
//   HeaderLine = { name: string; value: string }
//   DeliveryDetail = { ok: boolean; error: string; id: string; at: string; outcome: string; prompt: string; body: string; truncated: boolean; headers: HeaderLine[] }
//   HookView = { ok: boolean; error: string; id: string; url: string; source: string; stable: boolean }
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
type T1 = { "toast": T0; };
type T2 = boolean;
type T3 = { "slug": T0; "name": T0; "label": T0; "line": T0; "time": T0; "initials": T0; "color": T0; "shape": T0; "main": T2; "pinned": T2; "hidden": T2; "ready": T2; "working": T2; "unread": T2; "parentSlug": T0; "hideFromSidebar": T2; "subagent": T2; "prompt": T0; "inheritParentPrompt": T2; "notifications": T2; "aboveLimit": T2; };
type T4 = number;
type T5 = { "id": T0; "name": T0; "kind": T0; "size": T0; "url": T0; "width": T4; "height": T4; };
type T6 = { "label": T0; "value": T0; "key": T0; "warn": T0; };
type T7 = T0[];
type T8 = T7[];
type T9 = { "kind": T0; "text": T0; "head": T7; "rows": T8; "align": T7; };
type T10 = T5[];
type T11 = T6[];
type T12 = T9[];
type T13 = { "id": T0; "kind": T0; "mine": T2; "text": T0; "title": T0; "time": T0; "head": T2; "tail": T2; "variant": T0; "files": T10; "botSlug": T0; "botName": T0; "botColor": T0; "botShape": T0; "choices": T11; "placeholder": T0; "key": T0; "react": T0; "quote": T0; "tokens": T0; "resetAt": T0; "parts": T12; };
type T14 = { "id": T0; "text": T0; "missing": T2; };
type T15 = T13[];
type T16 = T14[];
type T17 = { "slug": T0; "rows": T15; "working": T2; "partial": T2; "readOnly": T2; "pins": T16; "threadId": T0; };
type T18 = { "slug": T0; "name": T0; "line": T0; "state": T0; "ready": T2; "working": T2; };
type T19 = { "id": T0; "name": T0; "text": T0; "from": T0; "parts": T12; };
type T20 = T18[];
type T21 = T19[];
type T22 = { "id": T0; "title": T0; "description": T0; "color": T0; "shape": T0; "members": T20; "messages": T21; "notifications": T2; "answerPolicy": T0; "answerPolicyEffective": T0; "chief": T0; };
type T23 = { "slug": T0; "name": T0; "color": T0; "shape": T0; };
type T24 = { "shape": T0; "color": T0; "state": T0; };
type T25 = T24[];
type T26 = { "kind": T0; "key": T0; "id": T0; "name": T0; "color": T0; "shape": T0; "faces": T25; "line": T0; "time": T0; "at": T0; "unread": T2; "working": T2; "ready": T2; "notifications": T2; };
type T27 = { "id": T0; "title": T0; "website": T0; "username": T0; "hasPassword": T2; "hints": T0; "icon": T0; };
type T28 = { "id": T0; "label": T0; };
type T29 = T28[];
type T30 = { "slug": T0; "name": T0; "recommended": T2; "effort": T0; "efforts": T29; };
type T31 = T30[];
type T32 = { "instanceId": T0; "name": T0; "account": T0; "models": T31; };
type T33 = T32[];
type T34 = { "ok": T2; "error": T0; "current": T0; "instanceId": T0; "effort": T0; "runtimeMode": T0; "currentName": T0; "providerName": T0; "effortLabel": T0; "efforts": T29; "providers": T33; };
type T35 = { "id": T0; "title": T0; "paused": T2; "schedule": T0; "next": T0; "failed": T2; "kind": T0; "outcome": T0; };
type T36 = T35[];
type T37 = { "ok": T2; "tasks": T36; "error": T0; };
type T38 = { "id": T0; "title": T0; "status": T0; "remote": T2; "threadId": T0; };
type T39 = T38[];
type T40 = { "ok": T2; "rows": T39; "error": T0; };
type T41 = { "day": T0; "label": T0; "short": T0; "tokens": T0; "input": T0; "output": T0; "cached": T0; "cacheWrite": T0; "turns": T0; "messages": T0; "px": T4; "spx": T4; };
type T42 = { "name": T0; "tokens": T0; "turns": T0; "share": T4; };
type T43 = { "slug": T0; "name": T0; "color": T0; "shape": T0; "tokens": T0; "turns": T0; "messages": T0; "share": T4; };
type T44 = T41[];
type T45 = T42[];
type T46 = T43[];
type T47 = { "ok": T2; "error": T0; "period": T0; "periodLabel": T0; "messages": T0; "attachments": T0; "turns": T0; "tokens": T0; "input": T0; "output": T0; "cached": T0; "cacheWrite": T0; "hit": T0; "sentence": T0; "days": T44; "rows": T44; "models": T45; "bots": T46; "empty": T2; "first": T0; "last": T0; };
type T48 = { "id": T0; "title": T0; "detail": T0; "url": T0; };
type T49 = T48[];
type T50 = { "ok": T2; "total": T4; "pictures": T49; "files": T49; "links": T49; };
type T51 = { "id": T0; "title": T0; "url": T0; };
type T52 = T51[];
type T53 = { "ok": T2; "tabs": T52; "image": T0; "error": T0; "selected": T0; };
type T54 = { "ok": T2; "error": T0; };
type T55 = { "ok": T2; "error": T0; "messageId": T0; };
type T56 = { "ok": T2; "error": T0; "text": T0; };
type T57 = { "authenticated": T2; "passwordRequired": T2; };
type T58 = { "mode": T0; "locked": T2; "restartRequired": T2; "canRestart": T2; "listening": T0; "urls": T0; "error": T0; "http": T2; };
type T59 = { "backend": T0; "root": T0; "backendLock": T0; "rootLock": T0; "restartRequired": T2; "canRestart": T2; "error": T0; "keySet": T2; "space": T0; "spaceLock": T0; };
type T60 = { "ok": T2; "error": T0; "challengeId": T0; };
type T61 = { "ok": T2; "error": T0; "space": T0; "spaceFound": T2; "shared": T2; "schemaReady": T2; };
type T62 = { "name": T0; "about": T0; "color": T0; "shape": T0; "first": T0; };
type T63 = { "ok": T2; "version": T0; "harness": T0; "connected": T2; };
type T64 = { "ok": T2; "installed": T2; "service": T2; "version": T0; "latest": T0; "updateAvailable": T2; "error": T0; };
type T65 = { "ok": T2; "version": T0; "error": T0; };
type T66 = { "ok": T2; "version": T0; };
type T67 = { "ok": T2; "id": T0; "error": T0; };
type T68 = { "id": T0; "name": T0; "collapsed": T2; "sort": T0; "items": T7; };
type T69 = T68[];
type T70 = { "userName": T0; "theme": T0; "defaultInstance": T0; "defaultModel": T0; "defaultEffort": T0; "defaultRuntimeMode": T0; "noticeEnabled": T2; "noticeSound": T2; "noticePreview": T0; "noticeWhen": T0; "noticeEnabledMissing": T2; "groupAnswerPolicy": T0; "maxRuntimeMode": T0; "codeBeta": T2; "sections": T69; "activityLevel": T0; "finalOnly": T2; };
type T71 = { "ok": T2; "text": T0; "error": T0; };
type T72 = { "visibilityState": T0; "hasFocus": T2; };
type T73 = { "available": T2; "state": T0; };
type T74 = { "ok": T2; "id": T0; "name": T0; "error": T0; };
type T75 = { "ok": T2; "id": T0; "name": T0; "url": T0; "seconds": T4; "error": T0; };
type T76 = { "ok": T2; "id": T0; "password": T0; "error": T0; };
type T77 = { "ok": T2; "error": T0; "messages": T4; "attachments": T4; "vaultWrites": T4; "iconFetches": T4; };
type T78 = { "stamp": T0; "texts": T7; "files": T7; "voices": T7; };
type T79 = { "locale": T0; };
type T80 = { "hover": T0; };
type T81 = { "id": T0; "title": T0; };
type T82 = T81[];
type T83 = { "chats": T82; };
type T84 = { "ok": T2; "error": T0; "threadId": T0; "messageId": T0; };
type T85 = T3[];
type T86 = { "computerEnlarged": T2; "computer": T53; "onComputer": T2; "holding": T2; "locked": T2; "bots": T85; "slug": T0; };
type T87 = T54 | null;
type T88 = { "system": T64; "password": T0; "signedIn": T87; "signInError": T0; "locked": T2; "signedInPending": T2; };
type T89 = T0 | null;
type T90 = { "body": T89; "data": T89; "tag": T0; "title": T0; };
type T91 = "notSupported" | "disabled" | "enabled";
type T92 = "notDetermined" | "denied" | "authorized" | "provisional" | "ephemeral";
type T93 = "notSupported" | "disabled" | "enabled";
type T94 = "notSupported" | "disabled" | "enabled";
type T95 = { "alertSetting": T91; "authorizationStatus": T92; "badgeSetting": T93; "soundSetting": T94; };
type T96 = T95 | null;
type T97 = "granted" | "prompt" | "denied" | "unknown";
type T98 = { "apple": T96; "available": T2; "badge": T2; "state": T97; };
type T99 = { "id": T0; "label": T0; "usedPercent": T4; "percent": T0; "resetLabel": T0; };
type T100 = T99[];
type T101 = { "instanceId": T0; "label": T0; "state": T0; "note": T0; "hint": T0; "windows": T100; };
type T102 = T101[];
type T103 = { "providers": T102; "sample": T2; "error": T0; "at": T0; };
type T104 = { "ok": T2; "error": T0; "cooling": T2; };
type T105 = { "id": T0; "label": T0; "permissions": T0; };
type T106 = T105[];
type T107 = { "loaded": T2; "enabled": T2; "endpoint": T0; "credentials": T106; };
type T108 = { "ok": T2; "error": T0; "token": T0; };
type T109 = { "id": T0; "label": T0; "local": T2; "status": T0; "laptop": T2; "lastSeenAt": T0; "t3Version": T0; "lastError": T0; "access": T0; };
type T110 = T109[];
type T111 = { "ok": T2; "machines": T110; "error": T0; };
type T112 = { "name": T0; "lastSeen": T0; };
type T113 = { "id": T0; "path": T0; };
type T114 = T112[];
type T115 = T113[];
type T116 = { "ok": T2; "state": T0; "lastSync": T0; "pending": T4; "devices": T114; "conflicts": T115; "needsImport": T2; "error": T0; };
type T117 = { "ok": T2; "started": T2; "push": T4; "pull": T4; "conflict": T4; "minutes": T4; "error": T0; };
type T118 = T61 | null;
type T119 = T26[];
type T120 = T98 | null;
type T121 = T60 | null;
type T122 = T65 | null;
type T123 = { "mcpStatus": T107; "mcpToken": T0; "mcpError": T0; "mcpPending": T2; "about": T63; "anytypeChecked": T118; "anytypePrepareError": T0; "anytypePreparedPending": T2; "anytypeError": T0; "anytypeFound": T2; "anytypeOn": T2; "anytypeShared": T2; "anytypeSpace": T0; "bots": T85; "canUpdate": T2; "convos": T119; "nameDraft": T0; "nameEditing": T2; "network": T58; "networkError": T0; "networkRestartError": T0; "networkRestarted": T87; "networkSet": T87; "newPassword": T0; "notice": T73; "noticeAsked": T120; "pairCode": T0; "pairConfirmError": T0; "pairConfirmed": T87; "pairError": T0; "pairStarted": T121; "pairStep": T0; "passwordError": T0; "passwordSet": T87; "prefs": T70; "prefsError": T0; "providers": T33; "limits": T103; "limitCooling": T2; "limitError": T0; "report": T47; "reportFailed": T2; "session": T57; "settingsDomain": T0; "shownBackend": T0; "spaceEditing": T2; "spaceName": T0; "spaceSaveError": T0; "spaceSaved": T87; "storage": T59; "sync": T116; "syncImportError": T0; "syncPreviewed": T2; "syncPreviewPush": T4; "syncPreviewPull": T4; "syncPreviewConflict": T4; "syncPreviewMinutes": T4; "storageCopied": T87; "storageCopyError": T0; "storageHint": T0; "storagePath": T0; "storageRestartError": T0; "storageRestarted": T87; "storageSaveError": T0; "storageSaved": T87; "storageTestError": T0; "storageTested": T87; "system": T64; "updateError": T0; "updateFrom": T0; "updateNeedsService": T2; "updateStarted": T122; "updateStuck": T2; "updateTo": T0; "usagePeriod": T0; "usageView": T0; "anytypeCheckedPending": T2; "networkRestartedPending": T2; "networkSetPending": T2; "noticeAskedPending": T2; "pairConfirmedPending": T2; "pairStartedPending": T2; "passwordSetPending": T2; "spaceSavedPending": T2; "storageCopiedPending": T2; "storageRestartedPending": T2; "storageSavedPending": T2; "storageTestedPending": T2; "syncNowPending": T2; "syncImportPending": T2; "syncResolvedPending": T2; "systemPending": T2; "updateStartedPending": T2; "machines": T111; "machinesFailed": T2; "machinesPending": T2; "diagnostics": T71; "diagAskError": T0; "diagAskPending": T2; };
type T124 = T67 | null;
type T125 = { "bots": T85; "canMakeGroup": T2; "groupAbout": T0; "groupChief": T0; "groupError": T0; "groupMade": T124; "groupMembers": T7; "groupName": T0; "groupPolicy": T0; "groupStep": T0; "pickableBots": T85; "groupMadePending": T2; };
type T126 = T62[];
type T127 = { "createError": T0; "createStep": T0; "createdPending": T2; "newColor": T0; "newFirst": T0; "newName": T0; "newPrompt": T0; "newShape": T0; "suggestions": T126; };
type T128 = { "accessibilityTextSize": T2; };
type T129 = { "prefersContrast": T0; };
type T130 = { "key": T0; "kind": T0; "text": T0; "slug": T0; "name": T0; "color": T0; "shape": T0; };
type T131 = { "key": T0; "text": T0; "start": T4; "n": T4; };
type T132 = { "start": T4; "query": T0; };
type T133 = { "text": T0; "cursor": T4; };
type T134 = { "start": T4; "end": T4; "slug": T0; "kind": T0; "name": T0; "color": T0; "shape": T0; };
type T135 = { "id": T0; "input": T0; "output": T0; "truncated": T2; "missing": T2; };
type T136 = T23[];
type T137 = T55 | null;
type T138 = { "answerError": T0; "answerText": T0; "answered": T87; "answeredPending": T2; "away": T2; "bots": T85; "chat": T17; "copiedMenu": T0; "groupPage": T0; "groupPanel": T2; "groupSaveError": T0; "mentionHits": T136; "mentionIdx": T4; "messageMenu": T0; "paintedWorkers": T20; "pointerFacts": T80; "prefs": T70; "reactFor": T0; "room": T22; "roomError": T0; "roomPending": T0; "roomRows": T15; "roomSent": T137; "roomText": T0; "wide": T2; "roomSentPending": T2; "roomFailed": T2; };
type T139 = { "rows": T15; "working": T2; "draft": T0; "canSend": T2; "pending": T7; "attachError": T0; "attachedPending": T2; "sentPending": T2; "replyTo": T0; "replyQuote": T0; "voicePreview": T0; "voiceError": T0; "voiceHold": T0; "voiceId": T0; "recording": T2; "voiceSentPending": T2; "answerText": T0; "answerError": T0; "answering": T2; "canMerge": T2; "name": T0; "bots": T85; "slug": T0; "pins": T16; "thread": T0; "hover": T2; "reactFor": T0; "messageMenu": T0; "copiedMenu": T0; "empty": T2; "failed": T2; "loading": T2; };
type T140 = { "enabled": T2; "cap": T0; "when": T0; "mode": T0; "effort": T0; "inherit": T2; "modelInstance": T0; "modelSlug": T0; "anyModel": T2; "modelList": T0; "modes": T0; "efforts": T0; };
type T141 = { "ok": T2; "error": T0; "lasting": T140; "once": T140; };
type T142 = { "policy": T141; "failed": T2; "pending": T2; "saving": T2; "error": T0; "seeded": T2; "whenLasting": T0; "whenOnce": T0; "capLasting": T0; "capOnce": T0; };
type T143 = { "ok": T2; "path": T0; "text": T0; "revision": T0; "truncated": T2; };
type T144 = { "ok": T2; "error": T0; "conflict": T2; "text": T0; "revision": T0; };
type T145 = { "file": T143; "failed": T2; "pending": T2; "editing": T2; "draft": T0; "conflict": T2; "serverText": T0; "serverRevision": T0; "error": T0; "saving": T2; };
type T146 = { "id": T0; "label": T0; "status": T0; "access": T0; "selectable": T2; };
type T147 = T146[];
type T148 = { "ok": T2; "selected": T0; "localLabel": T0; "links": T147; "waiting": T2; "error": T0; };
type T149 = { "path": T0; "insertions": T4; "deletions": T4; };
type T150 = { "number": T4; "title": T0; "url": T0; "draft": T2; "checks": T0; "review": T0; "updated": T0; };
type T151 = T149[];
type T152 = T150[];
type T153 = { "enabled": T2; "signal": T2; "loaded": T2; "error": T0; "branch": T0; "defaultBranch": T2; "ahead": T4; "behind": T4; "hasRemote": T2; "provider": T0; "baseUrl": T0; "insertions": T4; "deletions": T4; "fileTotal": T4; "files": T151; "linkedNumber": T4; "linkedTitle": T0; "linkedUrl": T0; "prs": T152; };
type T154 = { "ok": T2; "error": T0; "id": T0; "url": T0; "source": T0; "stable": T2; };
type T155 = { "ok": T2; "error": T0; "outcome": T0; };
type T156 = { "type": T0; "kind": T0; "title": T0; "prompt": T0; "preset": T0; "secret": T0; "f1Path": T0; "f1Op": T0; "f1Value": T0; "f2Path": T0; "f2Op": T0; "f2Value": T0; "replay": T0; "age": T0; "enable": T2; "savedId": T0; "url": T0; "source": T0; "stable": T2; "error": T0; "busy": T2; "testBody": T0; "testOutcome": T0; "testError": T0; "testBusy": T2; };
type T157 = { "id": T0; "at": T0; "method": T0; "bytes": T0; "outcome": T0; };
type T158 = T157[];
type T159 = { "ok": T2; "error": T0; "rows": T158; };
type T160 = { "name": T0; "value": T0; };
type T161 = T160[];
type T162 = { "ok": T2; "error": T0; "id": T0; "at": T0; "outcome": T0; "prompt": T0; "body": T0; "truncated": T2; "headers": T161; };
type T163 = { "ok": T2; "error": T0; "id": T0; "url": T0; "source": T0; "stable": T2; };
type T164 = { "routineId": T0; "url": T0; "source": T0; "stable": T2; "error": T0; "busy": T2; "openId": T0; "list": T159; "listDown": T2; "listPending": T2; "detailOpen": T2; "detail": T162; "detailDown": T2; "detailPending": T2; };
type T165 = T74 | null;
type T166 = { "system": T64; "models": T34; "modelError": T0; "shownRows": T15; "searchNote": T0; "away": T2; "answerText": T0; "chatSearching": T2; "messageQuery": T0; "reactFor": T0; "messageMenu": T0; "copiedMenu": T0; "replyTo": T0; "replyQuote": T0; "childThread": T0; "sent": T137; "answered": T87; "attached": T165; "pendingText": T0; "attachError": T0; "answerError": T0; "paintedWorkers": T20; "draft": T0; "mentionIdx": T4; "mentionHits": T136; "usagePeriod": T0; "usageView": T0; "report": T47; "childModels": T34; "routines": T37; "lineage": T40; "shelf": T50; "voiceRecorder": T0; "voiceError": T0; "voiceSent": T87; "voiceHold": T0; "voiceId": T0; "voicePreview": T0; "wide": T2; "panelOpen": T2; "panelTab": T0; "panelPage": T0; "subSlug": T0; "subName": T0; "subShape": T0; "subColor": T0; "subPrompt": T0; "subInherit": T2; "subCreated": T87; "subSaved": T87; "botError": T0; "subError": T0; "subSaveError": T0; "childRemoveError": T0; "heldTab": T0; "computerTab": T0; "computer": T53; "controlError": T0; "openError": T0; "openingBrowser": T2; "closeError": T0; "closingBrowser": T2; "driveError": T0; "holding": T2; "pointerFacts": T80; "bots": T85; "page": T72; "chat": T17; "slug": T0; "pendingAttaches": T7; "canSend": T2; "chatFailed": T2; "chatPending": T2; "answeredPending": T2; "attachedPending": T2; "voiceSentPending": T2; "sentPending": T2; "modelsFailed": T2; "routinesFailed": T2; "lineageFailed": T2; "reportFailed": T2; "shelfFailed": T2; "computerFailed": T2; "subCreatedPending": T2; "subSavedPending": T2; "childModelsFailed": T2; "linkDraft": T2; "linkLocal": T2; "limits": T102; "limitFocus": T0; "switchBot": T0; "switchFrom": T0; "switchTo": T0; "modelSavedPending": T2; "codeOn": T2; "code": T153; };
type T167 = { "prefs": T70; "botQuery": T0; "searching": T2; "hiddenOpen": T2; "rowMenu": T0; "firstRun": T2; "visibleBots": T85; "tiles": T85; "hiddenBots": T85; "newColor": T0; "newShape": T0; "wide": T2; "plusOpen": T2; "bots": T85; "convos": T119; "sectionHomes": T7; "chat": T17; "slug": T0; "query": T0; "roomId": T0; "convoName": T0; "botsFailed": T2; };
type T168 = T27[];
type T169 = { "vaultQuery": T0; "vault": T168; "vquery": T0; "shownVault": T168; "query": T0; "vaultFailed": T2; "vaultPending": T2; };
type T170 = { "permanentlyDenied": T2; "shouldShowRationale": T2; };
type T171 = T170 | null;
type T172 = "notDetermined" | "restricted" | "denied" | "authorized";
type T173 = { "authorizationStatus": T172; };
type T174 = T173 | null;
type T175 = "granted" | "prompt" | "denied" | "unknown";
type T176 = { "android": T171; "apple": T174; "available": T2; "state": T175; };
type T177 = T4 | null;
type T178 = { "max": T4; "min": T4; "step": T177; };
type T179 = { "refused": T89; };
type T180 = "audioinput" | "videoinput" | "audiooutput";
type T181 = { "deviceId": T0; "facingMode": T89; "groupId": T0; "kind": T180; "label": T0; };
type T182 = T181[];
type T183 = { "devices": T182; };
type T184 = { "cameraId": T89; "isClientSilenced": T2; "lensFacing": T89; };
type T185 = T184 | null;
type T186 = { "deviceType": T89; "portName": T89; "portType": T89; "position": T89; };
type T187 = T186 | null;
type T188 = T178 | null;
type T189 = T7 | null;
type T190 = T2 | null;
type T191 = { "aspectRatio": T188; "facingMode": T189; "frameRate": T188; "height": T188; "torch": T190; "width": T188; "zoom": T188; };
type T192 = T191 | null;
type T193 = "audio" | "video";
type T194 = "idle" | "interrupted" | "background" | "silenced" | "in-use" | "multitasking" | "system-pressure";
type T195 = T194 | null;
type T196 = "live" | "ended";
type T197 = { "aspectRatio": T177; "autoGainControl": T190; "channelCount": T177; "deviceId": T89; "echoCancellation": T190; "facingMode": T89; "frameRate": T177; "groupId": T89; "height": T177; "latency": T177; "noiseSuppression": T190; "resizeMode": T89; "sampleRate": T177; "sampleSize": T177; "torch": T190; "width": T177; "zoom": T177; };
type T198 = { "android": T185; "apple": T187; "capabilities": T192; "enabled": T2; "id": T0; "kind": T193; "label": T0; "muteReason": T195; "muted": T2; "readyState": T196; "settings": T197; };
type T199 = T198[];
type T200 = { "active": T2; "id": T0; "tracks": T199; };
type T201 = { "aspectRatio": T177; "autoGainControl": T190; "channelCount": T177; "deviceId": T89; "echoCancellation": T190; "facingMode": T89; "frameRate": T177; "groupId": T89; "height": T177; "latency": T177; "noiseSuppression": T190; "resizeMode": T89; "sampleRate": T177; "sampleSize": T177; "torch": T190; "width": T177; "zoom": T177; };
type T202 = { "permanentlyDenied": T2; "shouldShowRationale": T2; };
type T203 = T202 | null;
type T204 = "undetermined" | "denied" | "granted" | "notDetermined" | "restricted" | "authorized";
type T205 = { "recordPermission": T204; };
type T206 = T205 | null;
type T207 = "granted" | "prompt" | "denied" | "unknown";
type T208 = { "android": T203; "apple": T206; "available": T2; "state": T207; };
type T209 = { "x": T4; "y": T4; };
type T210 = T4[];
type T211 = { "audioLevel": T4; "levels": T210; "totalAudioEnergy": T4; "totalSamplesDuration": T4; };
type T212 = "requested" | "interrupted" | "media-reset" | "background" | "track-ended" | "error" | "session-ended";
type T213 = T212 | null;
type T214 = { "duration": T4; "mimeType": T0; "path": T0; "size": T4; };
type T215 = T214 | null;
type T216 = "inactive" | "recording" | "paused";
type T217 = { "audioBitsPerSecond": T177; "duration": T4; "error": T89; "id": T0; "mimeType": T0; "path": T0; "reason": T213; "result": T215; "state": T216; };
type T218 = { "supported": T2; "type": T0; };
type T219 = { "bot": T0; "prompt": T0; "present": T2; "bad": T2; };
type T220 = { "origin": T0; "local": T2; };
type T221 = "passive" | "active" | "timeSensitive";
type T222 = T221 | null;
type T223 = { "interruptionLevel": T222; "relevanceScore": T177; "subtitle": T89; "targetContentIdentifier": T89; "threadIdentifier": T89; };
type T224 = T223 | null;
type T225 = { "body": T89; "data": T89; "navigate": T89; "showTrigger": T177; "silent": T190; "tag": T89; "title": T0; "apple": T224; };
type T226 = { "tag": T0; };
type T227 = { "contents": T177; };
type T228 = "mic" | "camcorder" | "voiceRecognition" | "voiceCommunication" | "unprocessed" | "voicePerformance";
type T229 = T228 | null;
type T230 = { "audioSource": T229; "privacySensitive": T190; };
type T231 = "mixWithOthers" | "duckOthers" | "allowBluetoothHFP" | "allowBluetoothA2DP" | "defaultToSpeaker" | "bluetoothHighQualityRecording" | "farFieldInput" | "overrideMutedMicrophoneInterruption";
type T232 = T231[];
type T233 = T232 | null;
type T234 = "builtInWideAngleCamera" | "builtInUltraWideCamera" | "builtInTelephotoCamera" | "builtInDualCamera" | "builtInDualWideCamera" | "builtInTripleCamera" | "builtInTrueDepthCamera" | "builtInLiDARDepthCamera" | "continuityCamera" | "deskViewCamera" | "external";
type T235 = T234 | null;
type T236 = "default" | "measurement" | "voiceChat" | "videoChat" | "videoRecording";
type T237 = T236 | null;
type T238 = "stereo" | "cardioid" | "omnidirectional";
type T239 = T238 | null;
type T240 = { "categoryOptions": T233; "deviceType": T235; "inputDataSource": T89; "mode": T237; "polarPattern": T239; "preferredIOBufferDuration": T177; "prefersNoInterruptionsFromSystemAlerts": T190; };
type T241 = T201 | null;
type T242 = T230 | null;
type T243 = T240 | null;
type T244 = { "audio": T190; "audioConstraints": T241; "video": T190; "videoConstraints": T241; "android": T242; "apple": T243; };
type T245 = "builtInWideAngleCamera" | "builtInUltraWideCamera" | "builtInTelephotoCamera" | "builtInDualCamera" | "builtInDualWideCamera" | "builtInTripleCamera" | "builtInTrueDepthCamera" | "builtInLiDARDepthCamera" | "continuityCamera" | "deskViewCamera" | "external";
type T246 = T245 | null;
type T247 = "locked" | "autoExpose" | "continuousAutoExposure";
type T248 = T247 | null;
type T249 = T209 | null;
type T250 = "locked" | "autoFocus" | "continuousAutoFocus";
type T251 = T250 | null;
type T252 = { "deviceType": T246; "exposureMode": T248; "exposurePointOfInterest": T249; "focusMode": T251; "focusPointOfInterest": T249; };
type T253 = T252 | null;
type T254 = { "aspectRatio": T177; "facingMode": T89; "frameRate": T177; "height": T177; "resizeMode": T89; "torch": T190; "track": T0; "width": T177; "zoom": T177; "apple": T253; };
type T255 = { "track": T0; };
type T256 = { "enabled": T2; "track": T0; };
type T257 = { "track": T0; };
type T258 = "aac" | "heAac" | "opus";
type T259 = T258 | null;
type T260 = "mpeg4" | "ogg" | "webm";
type T261 = T260 | null;
type T262 = { "audioEncoder": T259; "outputFormat": T261; };
type T263 = "min" | "low" | "medium" | "high" | "max";
type T264 = T263 | null;
type T265 = "constant" | "longTermAverage" | "variableConstrained" | "variable";
type T266 = T265 | null;
type T267 = "aac" | "alac" | "linearPCM" | "opus";
type T268 = T267 | null;
type T269 = { "audioQuality": T264; "bitRateStrategy": T266; "formatID": T268; };
type T270 = "constant" | "variable";
type T271 = T270 | null;
type T272 = T262 | null;
type T273 = T269 | null;
type T274 = { "audioBitrateMode": T271; "audioBitsPerSecond": T177; "mimeType": T89; "path": T0; "stream": T0; "android": T272; "apple": T273; };
type T275 = { "recorder": T0; };
type T276 = { "recorder": T0; };
type T277 = { "recorder": T0; };
type T278 = { "interval": T177; "recorder": T0; };
type T279 = { "type": T0; };
type T280 = { "history": T177; "interval": T177; "track": T0; };
type T281 = { "slug": T0; "id": T0; };
type T282 = { "id": T4; "name": T0; "url": T0; "tab": T0; "params": T281; "fragment": T0; };
type T283 = T282[];
type T284 = { "name": T0; "stack": T283; };
type T285 = T284[];
type T286 = { "tab": T0; "tabs": T285; "next": T4; };
type T287 = null;
type T288 = T104 | null;
type T289 = T56 | null;
type T290 = T76 | null;
type T291 = T66 | null;
type T292 = T117 | null;
type T293 = T108 | null;
type T294 = T144 | null;
type T295 = T163 | null;
type T296 = T154 | null;
type T297 = T155 | null;
type T298 = T84 | null;
type T299 = T200 | null;
type T300 = T217 | null;
type T301 = T198 | null;
type T302 = T75 | null;
type T303 = { "key": T0; "shiftKey": T2; "ctrlKey": T2; "altKey": T2; "metaKey": T2; "code": T0; "repeat": T2; "isComposing": T2; };
type T304 = { "offsetX": T4; "offsetY": T4; "button": T4; "buttons": T4; "pressure": T4; "pointerType": T0; "pointerId": T4; "clientX": T4; "clientY": T4; "shiftKey": T2; "ctrlKey": T2; "altKey": T2; "metaKey": T2; };
type T305 = { "offsetX": T4; "offsetY": T4; "deltaX": T4; "deltaY": T4; "deltaMode": T4; "shiftKey": T2; "ctrlKey": T2; "altKey": T2; "metaKey": T2; };
type T306 = { "text": T0; "files": T7; };
type T307 = { "value": T0; "checked": T2; "selectionStart": T4; "selectionEnd": T4; "selectionDirection": T0; "inputType": T0; "data": T89; "cancelable": T2; "isComposing": T2; };
type T308 = { "path": T0; "name": T0; "type": T0; "size": T4; "width": T177; "height": T177; "duration": T177; };
type T309 = T308[];
type T310 = { "offsetX": T4; "offsetY": T4; "files": T7; "shiftKey": T2; "ctrlKey": T2; "altKey": T2; "metaKey": T2; };
type T311 = { "scrollLeft": T4; "scrollTop": T4; "scrollWidth": T4; "scrollHeight": T4; "clientWidth": T4; "clientHeight": T4; "firstVisibleKey": T89; "firstVisibleOffset": T4; };
type T312 = { "target": T0; "relatedTarget": T89; };

/** Sources called by compiled mutation sends. */
export type MutationSource = "askDiagnostics" | "checkAnytype" | "closeComputer" | "computerControl" | "computerInput" | "confirmPairing" | "copyStorage" | "createBot" | "createGroup" | "createManagementMcpCredential" | "createSubagent" | "deleteBot" | "deleteGroup" | "deleteLogin" | "dropVoice" | "enableNotifications" | "hideBot" | "importSync" | "makeMainBot" | "markRoomSeen" | "markSeen" | "mergeSide" | "noteUnread" | "openComputer" | "openSide" | "pinBot" | "pinMessage" | "prepareAnytype" | "reactTo" | "readHealth" | "readHook" | "recolourBot" | "refreshLimits" | "renameBot" | "reshapeBot" | "resolveConflict" | "respondRequest" | "restartGateway" | "restartStorage" | "revealPassword" | "revokeManagementMcpCredential" | "rotateHook" | "runSyncNow" | "saveBotMemory" | "saveBotPrompt" | "saveDrafts" | "saveHelperPolicy" | "saveLogin" | "saveRoutine" | "saveSections" | "saveSpace" | "saveStorage" | "saveSubagent" | "saveWebhookRoutine" | "sendMessage" | "sendRoom" | "sendVoice" | "setActivityLevel" | "setBotNotifications" | "setComputerPlace" | "setDefaultMode" | "setDefaultModel" | "setFinalOnly" | "setGroupAnswerPolicy" | "setManagementMcp" | "setMaxMode" | "setModel" | "setNetwork" | "setNotifications" | "setPassword" | "setTheme" | "setUserName" | "signIn" | "signOut" | "stageVoice" | "startPairing" | "startUpdate" | "stopRun" | "testStorage" | "testWebhookRoutine" | "updateGroup" | "uploadAttachment";

export interface SourceMap {
  "askDiagnostics": { args: [T0]; result: T54 };
  "checkAnytype": { args: []; result: T61 };
  "closeComputer": { args: [T0]; result: T54 };
  "computerControl": { args: [T0, T0, T0]; result: T54 };
  "computerInput": { args: [T0, T0, T4, T4, T4, T4, T4, T0, T4, T4]; result: T56 };
  "confirmPairing": { args: [T0, T0]; result: T54 };
  "copyStorage": { args: [T0]; result: T54 };
  "createBot": { args: [T0, T0, T0, T0, T0]; result: T67 };
  "createGroup": { args: [T0, T0, T0, T0, T0, T0, T0]; result: T67 };
  "createManagementMcpCredential": { args: [T0, T0]; result: T108 };
  "createSubagent": { args: [T0, T0, T0, T0, T0, T2]; result: T54 };
  "deleteBot": { args: [T0]; result: T54 };
  "deleteGroup": { args: [T0]; result: T54 };
  "deleteLogin": { args: [T0]; result: T54 };
  "dropVoice": { args: [T0, T0]; result: T54 };
  "enableNotifications": { args: []; result: T54 };
  "hideBot": { args: [T0, T2]; result: T54 };
  "importSync": { args: [T2]; result: T117 };
  "loadAbout": { args: []; result: T63 };
  "loadActivity": { args: [T0, T0, T0, T0]; result: T135 };
  "loadBots": { args: [T0]; result: T85 };
  "loadChat": { args: [T0, T0, T0]; result: T17 };
  "loadCode": { args: [T0, T0, T0]; result: T153 };
  "loadComputer": { args: [T0, T0, T0, T0, T0]; result: T53 };
  "loadComputerPlace": { args: [T0, T0, T0]; result: T148 };
  "loadConvos": { args: [T0, T0, T0]; result: T119 };
  "loadDeliveries": { args: [T0, T0, T0, T0]; result: T159 };
  "loadDelivery": { args: [T0, T0, T0, T0, T0]; result: T162 };
  "loadDiagnostics": { args: [T0]; result: T71 };
  "loadDrafts": { args: []; result: T78 };
  "loadHelperPolicy": { args: [T0, T0, T0]; result: T141 };
  "loadLimits": { args: [T0, T0]; result: T103 };
  "loadLineage": { args: [T0, T0, T0]; result: T40 };
  "loadMachines": { args: [T0, T0]; result: T111 };
  "loadManagementMcp": { args: [T0]; result: T107 };
  "loadMemory": { args: [T0, T0]; result: T143 };
  "loadModels": { args: [T0, T0, T0]; result: T34 };
  "loadNetwork": { args: [T0]; result: T58 };
  "loadNotice": { args: []; result: T73 };
  "loadPageOrigin": { args: []; result: T220 };
  "loadPrefs": { args: [T0]; result: T70 };
  "loadProviders": { args: []; result: T33 };
  "loadRoom": { args: [T0]; result: T22 };
  "loadRoutines": { args: [T0, T0, T0]; result: T37 };
  "loadSession": { args: [T0]; result: T57 };
  "loadShelf": { args: [T0, T0]; result: T50 };
  "loadSideChats": { args: [T0]; result: T83 };
  "loadStorage": { args: [T0]; result: T59 };
  "loadSuggestions": { args: []; result: T126 };
  "loadSync": { args: [T0, T0]; result: T116 };
  "loadSystem": { args: [T0]; result: T64 };
  "loadUsage": { args: [T0, T0]; result: T77 };
  "loadUsageReport": { args: [T0, T0, T0, T0]; result: T47 };
  "loadVault": { args: []; result: T168 };
  "makeMainBot": { args: [T0]; result: T54 };
  "markRoomSeen": { args: [T0]; result: T54 };
  "markSeen": { args: [T0]; result: T54 };
  "mergeSide": { args: [T0, T0]; result: T54 };
  "noteUnread": { args: [T0, T2, T2, T0, T2, T2, T2, T0, T0, T0, T0]; result: T54 };
  "openComputer": { args: [T0]; result: T54 };
  "openSide": { args: [T0, T0]; result: T84 };
  "pinBot": { args: [T0, T2]; result: T54 };
  "pinMessage": { args: [T0, T0, T0, T0]; result: T54 };
  "prepareAnytype": { args: []; result: T54 };
  "reactTo": { args: [T0, T0, T0, T0]; result: T54 };
  "readHealth": { args: []; result: T66 };
  "readHook": { args: [T0, T0, T0]; result: T163 };
  "readPromptLink": { args: [T0]; result: T219 };
  "recolourBot": { args: [T0, T0]; result: T54 };
  "refreshLimits": { args: []; result: T104 };
  "renameBot": { args: [T0, T0]; result: T54 };
  "reshapeBot": { args: [T0, T0]; result: T54 };
  "resolveConflict": { args: [T0, T0]; result: T54 };
  "respondRequest": { args: [T0, T0, T0, T0, T0, T0, T0]; result: T54 };
  "restartGateway": { args: []; result: T54 };
  "restartStorage": { args: []; result: T54 };
  "revealPassword": { args: [T0]; result: T76 };
  "revokeManagementMcpCredential": { args: [T0]; result: T108 };
  "rotateHook": { args: [T0, T0, T0]; result: T163 };
  "runSyncNow": { args: []; result: T54 };
  "saveBotMemory": { args: [T0, T0, T0]; result: T144 };
  "saveBotPrompt": { args: [T0, T0]; result: T54 };
  "saveDrafts": { args: [T7, T7, T7]; result: T54 };
  "saveHelperPolicy": { args: [T0, T0, T2, T0, T0, T0, T0, T2, T0, T0, T2, T0, T0, T0]; result: T54 };
  "saveLogin": { args: [T0, T0, T0, T0, T0, T0]; result: T54 };
  "saveRoutine": { args: [T0, T0, T0]; result: T154 };
  "saveSections": { args: [T7]; result: T54 };
  "saveSpace": { args: [T0]; result: T54 };
  "saveStorage": { args: [T0, T0, T0]; result: T54 };
  "saveSubagent": { args: [T0, T0, T0, T0, T0, T2]; result: T54 };
  "saveWebhookRoutine": { args: [T0, T0, T0, T0, T0, T0, T0, T0, T0, T0, T0, T0, T0, T2]; result: T154 };
  "sendMessage": { args: [T0, T0, T0, T0, T0, T0, T0, T0]; result: T55 };
  "sendRoom": { args: [T0, T0, T0]; result: T55 };
  "sendVoice": { args: [T0, T4, T0, T0, T0]; result: T54 };
  "setActivityLevel": { args: [T0]; result: T54 };
  "setBotNotifications": { args: [T0, T2]; result: T54 };
  "setComputerPlace": { args: [T0, T0]; result: T54 };
  "setDefaultMode": { args: [T0]; result: T54 };
  "setDefaultModel": { args: [T0, T0, T0]; result: T54 };
  "setFinalOnly": { args: [T0]; result: T54 };
  "setGroupAnswerPolicy": { args: [T0]; result: T54 };
  "setManagementMcp": { args: [T2]; result: T108 };
  "setMaxMode": { args: [T0]; result: T54 };
  "setModel": { args: [T0, T0, T0, T0, T0, T0]; result: T54 };
  "setNetwork": { args: [T0]; result: T54 };
  "setNotifications": { args: [T0, T2, T0, T0]; result: T54 };
  "setPassword": { args: [T0]; result: T54 };
  "setTheme": { args: [T0]; result: T54 };
  "setUserName": { args: [T0]; result: T54 };
  "signIn": { args: [T0]; result: T54 };
  "signOut": { args: []; result: T54 };
  "stageVoice": { args: [T0, T0, T0, T4]; result: T75 };
  "startPairing": { args: []; result: T60 };
  "startUpdate": { args: []; result: T65 };
  "stopRun": { args: [T0, T0, T0]; result: T54 };
  "testStorage": { args: [T0]; result: T54 };
  "testWebhookRoutine": { args: [T0, T0, T0, T0]; result: T155 };
  "updateGroup": { args: [T0, T0, T0]; result: T54 };
  "uploadAttachment": { args: [T0, T0, T0, T0, T0]; result: T74 };
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
