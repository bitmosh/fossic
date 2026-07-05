/// <reference types="node" />

export declare class EventId {
  constructor(bytes: Uint8Array)
  static fromHex(hex: string): EventId
  toHex(): string
  toBytes(): Uint8Array
}

export interface StoredEvent {
  id: string
  streamId: string
  branch: string
  version: bigint
  timestampUs: number
  causationId?: string | null
  correlationId?: string | null
  eventType: string
  typeVersion: number
  payload: unknown
  externalId?: string | null
  indexedTags?: unknown | null
}

export interface AppendInput {
  streamId: string
  eventType: string
  payload: unknown
  typeVersion?: number | null
  causationId?: string | null
  correlationId?: string | null
  externalId?: string | null
  indexedTags?: unknown | null
}

export interface ReadQuery {
  streamId: string
  branch?: string | null
  fromVersion?: bigint | null
  toVersion?: bigint | null
  limit?: number | null
  eventTypeFilter?: string | null
}

export interface StreamInfo {
  id: string
  declaredBy: string
  declaredAt: number
  description?: string | null
}

export interface BranchInfo {
  id: string
  streamId: string
  parentId: string
  parentVersion: bigint
  description?: string | null
  createdAt: number
  lifecycle: string
  closedAt?: number | null
  closedReason?: string | null
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

export interface OpenOptions {
  encryption?: string | null
  checkpointMode?: string | null
  onFirstOpen?: string | null
  defaultMaxResults?: number | null
  defaultMaxBytes?: number | null
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

export interface SnapshotState {
  version: bigint
  stateBytes: Buffer
}

// ── TruncationCursor ──────────────────────────────────────────────────────────

export declare class TruncationCursor {
  toBytes(): Buffer
  static fromBytes(buf: Buffer): TruncationCursor
}

// ── SamplingMode ──────────────────────────────────────────────────────────────

export type SamplingModeValue =
  | { kind: 'exhaustive' }
  | { kind: 'breadthFirst'; maxPerLevel: number }
  | { kind: 'adaptive'; targetCount: number }

export declare const SamplingMode: {
  exhaustive(): SamplingModeValue
  breadthFirst(maxPerLevel: number): SamplingModeValue
  adaptive(targetCount: number): SamplingModeValue
}

// ── ReadOutcome ───────────────────────────────────────────────────────────────

export type TruncationReason = 'result_count' | 'byte_size'

export type ReadOutcome =
  | { kind: 'complete'; results: StoredEvent[] }
  | { kind: 'truncated'; results: StoredEvent[]; reason: TruncationReason; nextCursor: TruncationCursor | null }

// ── Async iterators ───────────────────────────────────────────────────────────

export declare class FossicRangeIter implements AsyncIterable<StoredEvent> {
  [Symbol.asyncIterator](): AsyncIterator<StoredEvent>
}

export declare class FossicCorrelationIter implements AsyncIterable<StoredEvent> {
  [Symbol.asyncIterator](): AsyncIterator<StoredEvent>
}

export declare class FossicCausationIter implements AsyncIterable<StoredEvent> {
  [Symbol.asyncIterator](): AsyncIterator<StoredEvent>
}

// ── FossicSubscription ────────────────────────────────────────────────────────

export declare class FossicSubscription implements AsyncIterable<StoredEvent> {
  next(): Promise<IteratorResult<StoredEvent>>
  unsubscribe(): void
  [Symbol.asyncIterator](): AsyncIterator<StoredEvent>
  [Symbol.asyncDispose](): Promise<void>
}

// ── Store ─────────────────────────────────────────────────────────────────────

export declare class Store {
  static open(path: string, options?: OpenOptions | null): Store

  // Stream management
  declareStream(streamId: string, declaredBy: string, description?: string | null): Promise<void>
  streams(): Promise<Array<StreamInfo>>
  streamExists(streamId: string): Promise<boolean>

  // Append
  append(append: AppendInput): Promise<EventId>
  appendBatch(appends: Array<AppendInput>): Promise<Array<EventId>>

  // Read
  readRange(query: ReadQuery): Promise<Array<StoredEvent>>
  readOne(eventId: EventId): Promise<StoredEvent | null>
  readByExternalId(streamId: string, externalId: string): Promise<StoredEvent | null>
  readByCorrelation(correlationId: EventId): Promise<Array<StoredEvent>>
  walkCausation(start: EventId, direction: string, maxDepth?: number | null): Promise<Array<StoredEvent>>

  // Bounded reads
  readRangeBounded(query: ReadQuery, maxResults?: number | null, maxBytes?: number | null, cursor?: TruncationCursor | null): Promise<ReadOutcome>
  readByCorrelationBounded(correlationId: EventId, maxResults?: number | null, maxBytes?: number | null, cursor?: TruncationCursor | null): Promise<ReadOutcome>
  walkCausationBounded(start: EventId, direction: string, maxDepth?: number | null, sampling?: SamplingModeValue | null, maxResults?: number | null, maxBytes?: number | null, cursor?: TruncationCursor | null): Promise<ReadOutcome>

  // Streaming iterators
  readRangeIter(query: ReadQuery): FossicRangeIter
  readByCorrelationIter(correlationId: EventId): FossicCorrelationIter
  walkCausationIter(start: EventId, direction: string, maxDepth?: number | null, sampling?: SamplingModeValue | null): FossicCausationIter

  // Branches
  listBranches(streamId: string): Promise<Array<BranchInfo>>
  createBranch(b: CreateBranch): Promise<void>
  promoteBranch(streamId: string, branchId: string): Promise<void>
  markBranchDeadEnd(streamId: string, branchId: string): Promise<void>

  // Subscription
  subscribe(query: SubscribeQuery): FossicSubscription

  // Cursor
  getCursor(consumerId: string, streamId: string, branch: string): Promise<bigint | null>
  setCursor(consumerId: string, streamId: string, branch: string, version: bigint): Promise<void>

  // Snapshot primitives
  getSnapshotState(streamId: string, branch: string, reducerName: string, stateSchemaVersion: number): Promise<SnapshotState | null>
  writeSnapshotState(streamId: string, branch: string, version: bigint, reducerName: string, reducerVersion: number, stateSchemaVersion: number, stateBytes: Buffer): Promise<SnapshotInfo>

  // JS-side reducer support (added by index.js)
  registerReducer(pattern: string, reducer: unknown): void
  readState(streamId: string, branch?: string): Promise<unknown>
  readStateAtVersion(streamId: string, branch: string, version: bigint, reducerName?: string): Promise<unknown>

  close(): void
}

export declare function fossicVersion(): string

export declare const FossicErrorCode: {
  readonly ReducerNotFound: 'ReducerNotFound'
  readonly ReducerError: 'ReducerError'
  readonly StreamNotDeclared: 'StreamNotDeclared'
  readonly GenericFailure: 'GenericFailure'
}

export declare class FossicError extends Error {
  code: string
}

export interface FossicReducer {
  name: string
  version: number
  stateSchemaVersion: number
  initialState(): Uint8Array
  apply(state: Uint8Array, event: StoredEvent): Uint8Array
}
