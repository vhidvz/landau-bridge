export type Authenticity = 'ORIGINAL' | 'FAKE';
export type ImportMode = 'QUANTITY' | 'ZERO';
export type Phase = 'PREFLIGHT' | 'FETCH' | 'AGGREGATE' | 'MATCH' | 'REVIEW' | 'PRICE' | 'WRITE' | 'REPRICE' | 'REPORT';

export interface BridgeError { kind: string; message: string; detail: string }

export interface SettingsView {
  landau_url: string; has_landau_token: boolean; store: string | null; store_name: string | null;
  supplier_user: string | null; has_supplier_password: boolean; default_authenticity: Authenticity;
  threshold: number; vat_percent: number; overwrite_manual: boolean; secret_backend: 'keyring' | 'file'; data_dir: string;
}
export interface SettingsPatch {
  landau_url?: string; landau_token?: string; store?: string; store_name?: string; supplier_user?: string;
  supplier_password?: string; default_authenticity?: Authenticity; threshold?: number; vat_percent?: number; overwrite_manual?: boolean;
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
  invoice_ids: number[];
}
export interface JobEventRow { id: number; at: string; level: 'INFO' | 'WARN' | 'ERROR'; phase: string; message: string; detail: unknown }
export interface JobDetail { job: JobRow; items: JobItemRow[]; events: JobEventRow[] }
export interface Dashboard {
  settings_complete: boolean; store_name: string | null; supplier_user: string | null; invoices_cached: number; invoices_unpaid: number;
  last_import: JobRow | null; last_refresh: JobRow | null; running: JobRow | null; pending_review: number; recent_errors: number; decided_matches: number;
}
export interface MatchRow {
  supplier: string; code: string; product_id: string | null; product_name: string | null; how: 'AUTO' | 'USER' | 'NONE';
  similarity: number | null; name_seen: string; decided_at: string;
}
export interface ErrorRow { id: number; at: string; job_id: number | null; kind: string; message: string; context: unknown }
export interface ReviewDecision {
  item_id: number; product_id?: string; product_name?: string; similarity?: number; none?: boolean; later?: boolean; authenticity?: Authenticity;
}
export interface Progress { job_id: number; phase: Phase; done: number; total: number; message: string }
export interface JobEvent { job_id: number; level: 'INFO' | 'WARN' | 'ERROR'; kind?: string; phase: string; message: string; detail?: unknown }
export interface JobDone { job_id: number; status: string }
