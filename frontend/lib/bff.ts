import type { ApiErrorBody, PolicyVersionView } from "./api-types";
import { getTabId } from "./tab";

/** An error response from the API, normalised from its JSON envelope. */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
    readonly fields: { path: string; message: string }[] = [],
    readonly retryAfter: number | null = null,
    readonly latest: PolicyVersionView | null = null,
    readonly attemptsLeft: number | null = null,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

/** fetch() against the BFF with this tab's id attached. */
export function bffFetch(path: string, init: RequestInit = {}): Promise<Response> {
  const headers = new Headers(init.headers);
  headers.set("X-Tab-Id", getTabId());
  if (init.body !== undefined && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }
  return fetch(`/api/bff/${path.replace(/^\//, "")}`, { ...init, headers, cache: "no-store" });
}

export async function toApiError(res: Response): Promise<ApiError> {
  const retry = Number(res.headers.get("retry-after"));
  const retryAfter = Number.isFinite(retry) && retry > 0 ? retry : null;
  let body: ApiErrorBody | null = null;
  try {
    body = (await res.json()) as ApiErrorBody;
  } catch {
    // Axum's plain-text rejections (bad path params, 405) carry no envelope.
  }
  const e = body?.error;
  return new ApiError(
    res.status,
    e?.code ?? (res.status === 401 ? "unauthorized" : "http_error"),
    e?.message ?? `Request failed (HTTP ${res.status}).`,
    e?.fields ?? [],
    retryAfter,
    e?.latest ?? null,
    typeof e?.attempts_left === "number" ? e.attempts_left : null,
  );
}

/** JSON request through the BFF. Throws `ApiError` on any non-2xx status; a 204 resolves to `undefined`. */
export async function api<T>(path: string, init: RequestInit & { json?: unknown } = {}): Promise<T> {
  const { json, ...rest } = init;
  const res = await bffFetch(path, json === undefined ? rest : { ...rest, body: JSON.stringify(json) });
  if (!res.ok) throw await toApiError(res);
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

export function isApiError(e: unknown, status?: number): e is ApiError {
  return e instanceof ApiError && (status === undefined || e.status === status);
}
