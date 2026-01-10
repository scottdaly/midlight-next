// Touch Optimization Utilities for Mobile

/**
 * Debounce function for touch/scroll events
 * Uses requestAnimationFrame for smooth performance
 */
export function debounceRAF<T extends (...args: unknown[]) => void>(
  fn: T
): (...args: Parameters<T>) => void {
  let rafId: number | null = null;

  return (...args: Parameters<T>) => {
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
    }

    rafId = requestAnimationFrame(() => {
      fn(...args);
      rafId = null;
    });
  };
}

/**
 * Throttle function with trailing edge
 */
export function throttle<T extends (...args: unknown[]) => void>(
  fn: T,
  limit: number
): (...args: Parameters<T>) => void {
  let lastCall = 0;
  let timeoutId: ReturnType<typeof setTimeout> | null = null;

  return (...args: Parameters<T>) => {
    const now = Date.now();

    if (now - lastCall >= limit) {
      lastCall = now;
      fn(...args);
    } else {
      // Schedule trailing call
      if (timeoutId) {
        clearTimeout(timeoutId);
      }
      timeoutId = setTimeout(() => {
        lastCall = Date.now();
        fn(...args);
      }, limit - (now - lastCall));
    }
  };
}

interface NavigatorWithMsMaxTouchPoints extends Navigator {
  msMaxTouchPoints?: number;
}

/**
 * Check if device supports touch
 */
export function isTouchDevice(): boolean {
  if (typeof window === 'undefined') return false;

  return (
    'ontouchstart' in window ||
    navigator.maxTouchPoints > 0 ||
    ((navigator as NavigatorWithMsMaxTouchPoints).msMaxTouchPoints ?? 0) > 0
  );
}

/**
 * Check if device is mobile based on screen size
 */
export function isMobileScreen(): boolean {
  if (typeof window === 'undefined') return false;
  return window.innerWidth < 768;
}

/**
 * Check if device is in portrait orientation
 */
export function isPortrait(): boolean {
  if (typeof window === 'undefined') return false;
  return window.innerHeight > window.innerWidth;
}

/**
 * Add passive event listener for better scroll performance
 */
export function addPassiveEventListener(
  element: HTMLElement | Window | Document,
  event: string,
  handler: EventListener,
  options?: AddEventListenerOptions
): () => void {
  const passiveOptions: AddEventListenerOptions = {
    passive: true,
    ...options,
  };

  element.addEventListener(event, handler, passiveOptions);

  return () => {
    element.removeEventListener(event, handler, passiveOptions);
  };
}

/**
 * Prevent pull-to-refresh on mobile (for app-like experience)
 */
export function preventPullToRefresh(element: HTMLElement): () => void {
  let startY = 0;

  const handleTouchStart = (e: TouchEvent) => {
    startY = e.touches[0].clientY;
  };

  const handleTouchMove = (e: TouchEvent) => {
    const y = e.touches[0].clientY;
    const scrollTop = element.scrollTop;

    // Prevent overscroll at the top
    if (scrollTop <= 0 && y > startY) {
      e.preventDefault();
    }
  };

  element.addEventListener('touchstart', handleTouchStart, { passive: true });
  element.addEventListener('touchmove', handleTouchMove, { passive: false });

  return () => {
    element.removeEventListener('touchstart', handleTouchStart);
    element.removeEventListener('touchmove', handleTouchMove);
  };
}

/**
 * Handle long press for context menu on touch devices
 */
export function onLongPress(
  element: HTMLElement,
  callback: (event: TouchEvent) => void,
  duration = 500
): () => void {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;
  let startEvent: TouchEvent | null = null;

  const handleTouchStart = (e: TouchEvent) => {
    startEvent = e;
    timeoutId = setTimeout(() => {
      if (startEvent) {
        callback(startEvent);
      }
    }, duration);
  };

  const handleTouchEnd = () => {
    if (timeoutId) {
      clearTimeout(timeoutId);
      timeoutId = null;
    }
    startEvent = null;
  };

  const handleTouchMove = (e: TouchEvent) => {
    if (!startEvent) return;

    // Cancel if moved more than 10px
    const dx = e.touches[0].clientX - startEvent.touches[0].clientX;
    const dy = e.touches[0].clientY - startEvent.touches[0].clientY;

    if (Math.sqrt(dx * dx + dy * dy) > 10) {
      handleTouchEnd();
    }
  };

  element.addEventListener('touchstart', handleTouchStart, { passive: true });
  element.addEventListener('touchend', handleTouchEnd, { passive: true });
  element.addEventListener('touchcancel', handleTouchEnd, { passive: true });
  element.addEventListener('touchmove', handleTouchMove, { passive: true });

  return () => {
    handleTouchEnd();
    element.removeEventListener('touchstart', handleTouchStart);
    element.removeEventListener('touchend', handleTouchEnd);
    element.removeEventListener('touchcancel', handleTouchEnd);
    element.removeEventListener('touchmove', handleTouchMove);
  };
}

/**
 * Detect swipe gestures
 */
export interface SwipeHandlers {
  onSwipeLeft?: () => void;
  onSwipeRight?: () => void;
  onSwipeUp?: () => void;
  onSwipeDown?: () => void;
}

export function onSwipe(
  element: HTMLElement,
  handlers: SwipeHandlers,
  threshold = 50
): () => void {
  let startX = 0;
  let startY = 0;

  const handleTouchStart = (e: TouchEvent) => {
    startX = e.touches[0].clientX;
    startY = e.touches[0].clientY;
  };

  const handleTouchEnd = (e: TouchEvent) => {
    if (!e.changedTouches.length) return;

    const endX = e.changedTouches[0].clientX;
    const endY = e.changedTouches[0].clientY;

    const dx = endX - startX;
    const dy = endY - startY;

    const absDx = Math.abs(dx);
    const absDy = Math.abs(dy);

    // Determine primary direction
    if (absDx > threshold || absDy > threshold) {
      if (absDx > absDy) {
        // Horizontal swipe
        if (dx > 0) {
          handlers.onSwipeRight?.();
        } else {
          handlers.onSwipeLeft?.();
        }
      } else {
        // Vertical swipe
        if (dy > 0) {
          handlers.onSwipeDown?.();
        } else {
          handlers.onSwipeUp?.();
        }
      }
    }
  };

  element.addEventListener('touchstart', handleTouchStart, { passive: true });
  element.addEventListener('touchend', handleTouchEnd, { passive: true });

  return () => {
    element.removeEventListener('touchstart', handleTouchStart);
    element.removeEventListener('touchend', handleTouchEnd);
  };
}

/**
 * Create optimized scroll handler
 */
export function createScrollHandler(
  callback: (scrollTop: number, scrollHeight: number, clientHeight: number) => void
): (event: Event) => void {
  const debouncedHandler = debounceRAF((event: unknown) => {
    const target = (event as Event).target as HTMLElement;
    callback(target.scrollTop, target.scrollHeight, target.clientHeight);
  });

  return (event: Event) => debouncedHandler(event);
}

/**
 * CSS for touch-friendly UI elements
 * Apply as classes or inline styles
 */
export const touchStyles = {
  // Minimum touch target size (48x48 as per WCAG)
  touchTarget: {
    minWidth: '48px',
    minHeight: '48px',
    padding: '12px',
  },

  // Prevent text selection on drag
  noSelect: {
    userSelect: 'none',
    WebkitUserSelect: 'none',
    WebkitTouchCallout: 'none',
  },

  // Smooth momentum scrolling on iOS
  smoothScroll: {
    WebkitOverflowScrolling: 'touch',
    overflowY: 'auto',
  },

  // Prevent tap highlight
  noTapHighlight: {
    WebkitTapHighlightColor: 'transparent',
  },
} as const;
