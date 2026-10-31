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
}
export interface JobEventRow { id: number; at: string; level: 'INFO' | 'WARN' | 'ERROR'; phase: string; message: string; detail: unknown }
export interface JobDetail { job: JobRow; items: JobItemRow[]; events: JobEventRow[] }
export interface Dashboard {
  settings_complete: boolean; store_name: string | null; supplier_user: string | null; invoices_cached: number; invoices_unpaid: number;
  last_import: JobRow | null; last_refresh: JobRow | null; running: JobRow | null; pending_review: number; recent_errors: number; decided_matches: number;
  appimage_offer: boolean;
}
export interface MatchRow {
  supplier: string; name_key: string; code: string; product_id: string | null; product_name: string | null; how: 'AUTO' | 'USER' | 'NONE';
  similarity: number | null; name_seen: string; priced_at: string | null; decided_at: string;
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
}
export interface Progress { job_id: number; phase: Phase; done: number; total: number; message: string }
export interface JobEvent { job_id: number; level: 'INFO' | 'WARN' | 'ERROR'; kind?: string; phase: string; message: string; detail?: unknown }
export interface JobDone { job_id: number; status: string }
