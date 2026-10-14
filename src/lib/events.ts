import { inTauri } from './api';

type Handler = (payload: never) => void;

const handlers = new Map<string, Set<Handler>>();
const nativeUnsubscribers = new Map<string, Array<() => void>>();
const nativeSubscribed = new Set<string>();

export function emitLocal(event: string, payload: unknown): void {
  const set = handlers.get(event);
  if (!set) return;
  for (const handler of [...set]) {
    (handler as (p: unknown) => void)(payload);
  }
}

if (typeof window !== 'undefined') {
  window.__devConsoleEmit = emitLocal;
}

/** Subscribe before performing the initial fetch; returns an unsubscribe function. */
export function on<T>(event: string, handler: (payload: T) => void): () => void {
  const set = handlers.get(event) ?? new Set<Handler>();
  const wrapped = handler as Handler;
  set.add(wrapped);
  handlers.set(event, set);

  if (inTauri() && !nativeSubscribed.has(event)) {
    nativeSubscribed.add(event);
    void import('@tauri-apps/api/event')
      .then(({ listen }) => listen(event, (message) => emitLocal(event, message.payload)))
      .then((unlisten) => {
        const list = nativeUnsubscribers.get(event) ?? [];
        list.push(unlisten);
        nativeUnsubscribers.set(event, list);
        if (!handlers.has(event)) void removeAll(event);
      })
      .catch((error: unknown) => console.error('event subscription failed', event, error));
  }

  return () => {
    const current = handlers.get(event);
    if (!current) return;
    current.delete(wrapped);
    if (current.size === 0) {
      handlers.delete(event);
      if (nativeSubscribed.has(event)) void removeAll(event);
    }
  };
}

function removeAll(event: string): Promise<void> {
  nativeSubscribed.delete(event);
  const list = nativeUnsubscribers.get(event) ?? [];
  nativeUnsubscribers.delete(event);
  for (const unlisten of list) {
    void Promise.resolve().then(unlisten).catch(() => undefined);
  }
  return Promise.resolve();
}
