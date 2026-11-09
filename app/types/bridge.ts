export type Authenticity = 'ORIGINAL' | 'FAKE';
export type ImportMode = 'QUANTITY' | 'ZERO';
export type Phase = 'PREFLIGHT' | 'FETCH' | 'AGGREGATE' | 'MATCH' | 'REVIEW' | 'PRICE' | 'WRITE' | 'REPRICE' | 'REPORT';

export interface BridgeError { kind: string; message: string; detail: string }

export interface SettingsView {
  landau_url: string; has_landau_token: boolean; store: string | null; store_name: string | null;
  supplier_user: string | null; has_supplier_password: boolean; default_authenticity: Authenticity;
  threshold: number; vat_percent: number; min_margin_percent: number; overwrite_manual: boolean; fetch_since_only: boolean;
  secret_backend: 'keyring' | 'file'; data_dir: string;
}
export type InstallKind = 'appimage' | 'nsis' | 'deb' | 'rpm' | 'unknown';
export type UpdateState =
  | { state: 'Idle' } | { state: 'Checking' } | { state: 'Available'; version: string }
  | { state: 'Downloading'; version: string; percent: number } | { state: 'Ready'; version: string }
  | { state: 'Installing'; version: string } | { state: 'Failed'; kind: string; message: string; at: string };
export interface UpdateView { current: string; state: UpdateState; kind: InstallKind; auto_update: boolean; last_check: string | null }
export interface SettingsPatch {
  landau_url?: string; landau_token?: string; store?: string; store_name?: string; supplier_user?: string;
  supplier_password?: string; default_authenticity?: Authenticity; threshold?: number; vat_percent?: number; min_margin_percent?: number; overwrite_manual?: boolean;
  fetch_since_only?: boolean; auto_update?: boolean;
}
export interface Store { id: string; name: string | null; kind: string | null; state: string | null; status: string | null }
export interface LandauStatus { business: string; business_name: string | null; approved: boolean; stores: Store[]; active_rules: number }

export interface JobRow {
  id: number; kind: 'IMPORT' | 'REFRESH'; mode: ImportMode | null; since_date: string | null; store: string;
  overwrite: boolean; status: string; phase: Phase; started_at: string; finished_at: string | null; summary: Record<string, unknown> | null;
}
export interface Candidate { id: string; name: string; alias: string | null; brand: string | null; score: number }
export interface Scored { candidate: Candidate; similarity: number }
export interface JobItemRow {
  id: number; job_id: number; code: string; name: string; qty: number; product_id: string | null; product_name: string | null;
  stock_id: string | null; authenticity: Authenticity | null; action: 'ATTACH' | 'PATCH' | 'SKIP' | null;
  qty_before: number | null; qty_after: number | null; cost: number | null; price_before: number | null; price_after: number | null;
  reprice_verdict: string | null; error_kind: string | null; error_msg: string | null; candidates: Scored[] | null; status: string;
  invoice_ids: number[]; available: boolean | null; note: string | null;
  /** Why the write table held the link (plan 130 R-k); null when it was not held. */
  reasons?: Reason[] | null;
}
export interface JobEventRow { id: number; at: string; level: 'INFO' | 'WARN' | 'ERROR'; phase: string; message: string; detail: unknown }
export interface JobDetail { job: JobRow; items: JobItemRow[]; events: JobEventRow[] }
export interface Dashboard {
  settings_complete: boolean; store_name: string | null; supplier_user: string | null; invoices_cached: number; invoices_unpaid: number;
  last_import: JobRow | null; last_refresh: JobRow | null; running: JobRow | null; pending_review: number; recent_errors: number; decided_matches: number;
  links_to_review: number; held_writes: number; appimage_offer: boolean;
}
export type LinkState = 'UNVERIFIED' | 'VALID' | 'REBUILT' | 'NEEDS_REVIEW' | 'STALE';
export interface MatchRow {
  supplier: string; name_key: string; code: string; product_id: string | null; product_name: string | null; how: 'AUTO' | 'USER' | 'NONE';
  similarity: number | null; name_seen: string; priced_at: string | null; decided_at: string;
  state: LinkState; state_reason: string | null; state_at: string | null; validated_job: number | null; link_version: number;
  product_brand: string | null; product_alias: string | null; confirmed_json: string | null;
}
/** A part ended import jobs left without a decision, with the candidates review showed then. */
export interface UndecidedRow {
  name_key: string; name_seen: string; code: string; job_id: number; seen_at: string; qty: number; times: number; candidates: Scored[];
}
export interface DecisionConflict { name_seen: string; code: string; mine_how: string; mine_product: string | null; file_how: string; file_product: string | null }
export interface DecisionMissing { name_seen: string; code: string; product_id: string; product_name: string | null }
export interface DecisionImportReport {
  total: number; added: number; unchanged: number; replaced: number; kept: DecisionConflict[]; missing: DecisionMissing[]; invalid: number;
}
export interface ErrorRow { id: number; at: string; job_id: number | null; kind: string; message: string; context: unknown }
export interface ReviewDecision {
  item_id: number; product_id?: string; product_name?: string; similarity?: number; none?: boolean; later?: boolean; authenticity?: Authenticity;
  /** Exactly the reasons a `ConfirmRequired` refusal named, once the seller confirmed them (R-d). */
  confirmed?: Reason[];
}

// ---- plan 130: evidence, the write table and the money around a pick ------------------------------
export type EvidenceState = 'CONSISTENT' | 'CONFLICT' | 'INSUFFICIENT';
export type Reason =
  | { kind: 'BELOW_THRESHOLD'; similarity: number; threshold: number }
  | { kind: 'TIED' }
  | { kind: 'ANOTHER_PASSES'; product_id: string }
  | { kind: 'NUMBER_CONFLICT'; lacking: string[] }
  | { kind: 'NUMBER_UNBACKED'; candidate: string[] }
  | { kind: 'WORDS_UNCOVERED'; words: string[] }
  | { kind: 'OPPOSITE_WORDS'; supplier: string; candidate: string }
  | { kind: 'PART_WORDS_MISSING'; words: string[] }
  | { kind: 'NO_BRAND_IN_NAME' }
  | { kind: 'SEVERAL_BRANDS_IN_NAME'; brands: string[] }
  | { kind: 'BRAND_UNSURE'; supplier: string }
  | { kind: 'CANDIDATE_WITHOUT_BRAND' }
  | { kind: 'BRAND_MISMATCH'; supplier: string; candidate: string }
  | { kind: 'DEFINITION_NOT_EXACT' };
export type Verdict = { verdict: 'WRITE' } | { verdict: 'REVIEW'; reasons: Reason[] } | { verdict: 'CONFIRM_THEN_WRITE'; reasons: Reason[] };
export type SupplierBrand = { kind: 'ONE'; brand: string; ambiguous: boolean } | { kind: 'SEVERAL'; brands: string[] } | { kind: 'NONE' };
export interface Evidence {
  numbers: { state: EvidenceState; unbacked: boolean; supplier: string[]; candidate: string[]; shared: string[]; lacking: string[] };
  words: { state: EvidenceState; uncovered: string[]; opposites: [string, string][]; missing: string[] };
  brand: { state: EvidenceState; supplier: SupplierBrand; candidate: string | null };
}
/** A figure from a read that may have failed: `UNKNOWN` carries the sentence, never a zero. */
export type Read<T> = { status: 'KNOWN'; value: T } | { status: 'UNKNOWN'; value: string };
export interface QuoteView {
  code: string; name: string; list_price: number; has_vat: boolean; site_vat: number | null; vat_setting: number;
  cost: number; available: boolean; renamed: boolean; read_at: string;
}
export interface ShelfView { id: string; inventory: number; cost: number | null; price: number | null; price_source: string | null; read_at: string }
export interface RuleView { scope: string | null; basis: string | null; value: number | null }
export interface CandidateView {
  name: string; code: string; product: Candidate; similarity: number; evidence: Evidence; verdict: Verdict; shelf: Read<ShelfView | null>;
  /** The cost PRICE would write on that row today (the unavailable-only-raises rule applied); null while unknown. */
  next_cost: number | null;
  /** An unavailable quote below the row's cost: the row keeps its cost. */
  cost_kept: boolean;
}
export interface Inspection {
  registry_read_at: string; quote: Read<QuoteView | null>; rules: Read<RuleView[]>; candidates: CandidateView[]; missing: string[];
}
export interface RowState { place: 'LIVE' | 'TRASH' | 'ABSENT'; stock_id: string | null; inventory: number | null; cost: number | null; updated_at: string | null }
/** A shelf write held until the seller settles it (plan 130 R-o). */
export interface HeldWrite {
  id: number; job_id: number; kind: 'IMPORT' | 'REFRESH'; store: string; product: string; authenticity: Authenticity; verb: string;
  snapshot: RowState; target_inventory: number | null; target_cost: number; items: number[]; status: string;
  outcome: { row?: RowState } | null; opened_at: string; closed_at: string | null;
  product_name: string | null; supplier_names: string[];
}
export interface Progress { job_id: number; phase: Phase; done: number; total: number; message: string }
export interface JobEvent { job_id: number; level: 'INFO' | 'WARN' | 'ERROR'; kind?: string; phase: string; message: string; detail?: unknown }
export interface JobDone { job_id: number; status: string }
