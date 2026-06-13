/* auto-generated type declarations for fossic Node.js binding */

export interface StoredEvent {
  id: string
  streamId: string
  branch: string
  /** Monotonic version number for this (stream, branch). */
  version: bigint
  timestampUs: number
  causationId: string | null
  correlationId: string | null
  eventType: string
  typeVersion: number
  /** Msgpack payload decoded to a JSON value. */
  payload: unknown
  externalId: string | null
  indexedTags: unknown | null
}

export interface Append {
  streamId: string
  eventType: string
  payload: unknown
  typeVersion?: number
  causationId?: string | null
  correlationId?: string | null
  externalId?: string | null
  indexedTags?: unknown | null
}

export interface ReadQuery {
  streamId: string
  branch?: string | null
  /** Inclusive lower bound. */
  fromVersion?: bigint | null
  /** Inclusive upper bound. */
  toVersion?: bigint | null
  limit?: number | null
  /** When set, only return events whose eventType matches exactly. */
  eventTypeFilter?: string | null
}

export interface StreamInfo {
  id: string
  declaredBy: string
  declaredAt: number
  description: string | null
}

export interface BranchInfo {
  id: string
  streamId: string
  parentId: string
  parentVersion: bigint
  description: string | null
  createdAt: number
  lifecycle: string
  closedAt: number | null
  closedReason: string | null
}

export interface SnapshotInfo {
  streamId: string
  branch: string
  version: bigint
  reducerName: string
  reducerVersion: number
  stateSchemaVersion: number
  createdAt: number
}

export interface SnapshotState {
  version: bigint
  stateBytes: Buffer
}

export interface OpenOptions {
  /** `"plaintext"` (default) or `"per_stream"`. */
  encryption?: string | null
  /** `"auto"` (default) | `"manual"`. */
  checkpointMode?: string | null
  /** `"create_if_missing"` (default) | `"fail_if_not_found"`. */
  onFirstOpen?: string | null
}

export interface SubscribeQuery {
  streamPattern: string
  branch?: string | null
  includeSystem?: boolean | null
  queueSize?: number | null
}

export interface CreateBranch {
  streamId: string
  branchId: string
  parentId?: string | null
  parentVersion: bigint
  description?: string | null
}

export declare class EventId {
  constructor(bytes: Uint8Array)
  static fromHex(hex: string): EventId
  toHex(): string
  toBytes(): Uint8Array
}

export declare class FossicSubscription {
  unsubscribe(): void
  isDegraded(): boolean
  next(): Promise<IteratorResult<StoredEvent>>
  [Symbol.asyncIterator](): AsyncIterator<StoredEvent>
  [Symbol.asyncDispose](): Promise<void>
}

export declare class Store {
  static open(path: string, options?: OpenOptions | null): Store

  declareStream(streamId: string, declaredBy: string, description?: string | null): Promise<void>
  streams(): Promise<StreamInfo[]>
  streamExists(streamId: string): Promise<boolean>

  append(append: Append): Promise<EventId>
  appendBatch(appends: Append[]): Promise<EventId[]>

  readRange(query: ReadQuery): Promise<StoredEvent[]>
  readOne(eventId: EventId): Promise<StoredEvent | null>
  readByExternalId(streamId: string, externalId: string): Promise<StoredEvent | null>
  readByCorrelation(correlationId: EventId): Promise<StoredEvent[]>
  walkCausation(start: EventId, direction: 'forward' | 'backward', maxDepth?: number | null): Promise<StoredEvent[]>

  listBranches(streamId: string): Promise<BranchInfo[]>
  createBranch(b: CreateBranch): Promise<void>
  promoteBranch(streamId: string, branchId: string): Promise<void>
  markBranchDeadEnd(streamId: string, branchId: string): Promise<void>

  subscribe(query: SubscribeQuery): FossicSubscription

  getCursor(consumerId: string, streamId: string, branch: string): Promise<bigint | null>
  setCursor(consumerId: string, streamId: string, branch: string, version: bigint): Promise<void>

  getSnapshotState(streamId: string, branch: string, reducerName: string, stateSchemaVersion: number): Promise<SnapshotState | null>
  writeSnapshotState(streamId: string, branch: string, version: bigint, reducerName: string, reducerVersion: number, stateSchemaVersion: number, stateBytes: Buffer): Promise<SnapshotInfo>

  close(): void
}
