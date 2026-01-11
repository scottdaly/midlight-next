// Performance utilities for stores

/**
 * Creates a debounced function that delays invoking func until after wait
 * milliseconds have elapsed since the last time the debounced function was invoked.
 */
export function debounce<T extends (...args: unknown[]) => void>(
  func: T,
  wait: number
): (...args: Parameters<T>) => void {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;

  return function (...args: Parameters<T>) {
    if (timeoutId) {
      clearTimeout(timeoutId);
    }
    timeoutId = setTimeout(() => {
      func(...args);
      timeoutId = null;
    }, wait);
  };
}

/**
 * Creates a throttled function that only invokes func at most once
 * per every wait milliseconds.
 */
export function throttle<T extends (...args: unknown[]) => void>(
  func: T,
  wait: number
): (...args: Parameters<T>) => void {
  let lastTime = 0;
  let timeoutId: ReturnType<typeof setTimeout> | null = null;

  return function (...args: Parameters<T>) {
    const now = Date.now();

    if (now - lastTime >= wait) {
      func(...args);
      lastTime = now;
    } else if (!timeoutId) {
      timeoutId = setTimeout(
        () => {
          func(...args);
          lastTime = Date.now();
          timeoutId = null;
        },
        wait - (now - lastTime)
      );
    }
  };
}

/**
 * Creates a memoized version of a function.
 * Results are cached based on the first argument (shallow equality).
 */
export function memoize<T extends (arg: unknown) => unknown>(
  func: T,
  maxCacheSize = 100
): T {
  const cache = new Map<unknown, unknown>();

  return function (arg: unknown) {
    if (cache.has(arg)) {
      return cache.get(arg);
    }

    const result = func(arg);

    // Limit cache size
    if (cache.size >= maxCacheSize) {
      const firstKey = cache.keys().next().value;
      if (firstKey !== undefined) {
        cache.delete(firstKey);
      }
    }

    cache.set(arg, result);
    return result;
  } as T;
}

/**
 * Batches multiple calls into a single callback after a delay.
 * Useful for accumulating changes before processing.
 */
export function batchCalls<T>(
  callback: (items: T[]) => void,
  delay = 100
): (item: T) => void {
  let batch: T[] = [];
  let timeoutId: ReturnType<typeof setTimeout> | null = null;

  return function (item: T) {
    batch.push(item);

    if (!timeoutId) {
      timeoutId = setTimeout(() => {
        const items = batch;
        batch = [];
        timeoutId = null;
        callback(items);
      }, delay);
    }
  };
}
