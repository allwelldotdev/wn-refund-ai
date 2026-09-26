const KEY = "tabId";

/**
 * A random id per browser tab (sessionStorage is per tab). The BFF keys the
 * session cookie by it, so every tab signs in separately (ADR-010). A
 * duplicated tab copies sessionStorage and therefore shares the session.
 */
export function getTabId(): string {
  let id = sessionStorage.getItem(KEY);
  if (!id) {
    id = crypto.randomUUID();
    sessionStorage.setItem(KEY, id);
  }
  return id;
}
