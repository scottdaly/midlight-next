// Performance Monitoring - Web Vitals and custom metrics

export interface PerformanceMetrics {
  // Core Web Vitals
  lcp?: number; // Largest Contentful Paint
  fid?: number; // First Input Delay
  cls?: number; // Cumulative Layout Shift
  fcp?: number; // First Contentful Paint
  ttfb?: number; // Time to First Byte

  // Custom metrics
  editorLoadTime?: number;
  syncLatency?: number;
  saveLatency?: number;

  // Navigation timing
  domContentLoaded?: number;
  loadComplete?: number;
}

type MetricCallback = (metrics: PerformanceMetrics) => void;

let metricsCallbacks: MetricCallback[] = [];
let collectedMetrics: PerformanceMetrics = {};

/**
 * Initialize performance monitoring
 */
export function initPerformanceMonitoring(): void {
  if (typeof window === 'undefined') return;

  // Collect navigation timing
  collectNavigationTiming();

  // Observe Largest Contentful Paint
  observeLCP();

  // Observe First Input Delay
  observeFID();

  // Observe Cumulative Layout Shift
  observeCLS();

  // Observe First Contentful Paint
  observeFCP();

  console.log('[Performance] Monitoring initialized');
}

/**
 * Register a callback to receive metrics
 */
export function onMetrics(callback: MetricCallback): () => void {
  metricsCallbacks.push(callback);

  // Send any already collected metrics
  if (Object.keys(collectedMetrics).length > 0) {
    callback(collectedMetrics);
  }

  return () => {
    metricsCallbacks = metricsCallbacks.filter((cb) => cb !== callback);
  };
}

/**
 * Report a metric
 */
function reportMetric(name: keyof PerformanceMetrics, value: number): void {
  collectedMetrics[name] = value;
  metricsCallbacks.forEach((cb) => cb({ [name]: value }));
}

/**
 * Collect navigation timing metrics
 */
function collectNavigationTiming(): void {
  // Wait for page load
  if (document.readyState === 'complete') {
    processNavigationTiming();
  } else {
    window.addEventListener('load', () => {
      // Small delay to ensure metrics are available
      setTimeout(processNavigationTiming, 0);
    });
  }
}

function processNavigationTiming(): void {
  const navigation = performance.getEntriesByType('navigation')[0] as PerformanceNavigationTiming;
  if (!navigation) return;

  reportMetric('ttfb', navigation.responseStart - navigation.requestStart);
  reportMetric('domContentLoaded', navigation.domContentLoadedEventEnd - navigation.startTime);
  reportMetric('loadComplete', navigation.loadEventEnd - navigation.startTime);
}

/**
 * Observe Largest Contentful Paint
 */
function observeLCP(): void {
  if (!('PerformanceObserver' in window)) return;

  try {
    const observer = new PerformanceObserver((list) => {
      const entries = list.getEntries();
      const lastEntry = entries[entries.length - 1] as PerformanceEntry & { startTime: number };
      if (lastEntry) {
        reportMetric('lcp', lastEntry.startTime);
      }
    });

    observer.observe({ type: 'largest-contentful-paint', buffered: true });
  } catch (e) {
    // LCP not supported
  }
}

/**
 * Observe First Input Delay
 */
function observeFID(): void {
  if (!('PerformanceObserver' in window)) return;

  try {
    const observer = new PerformanceObserver((list) => {
      const entries = list.getEntries() as (PerformanceEntry & { processingStart: number; startTime: number })[];
      const firstEntry = entries[0];
      if (firstEntry) {
        reportMetric('fid', firstEntry.processingStart - firstEntry.startTime);
      }
    });

    observer.observe({ type: 'first-input', buffered: true });
  } catch (e) {
    // FID not supported
  }
}

/**
 * Observe Cumulative Layout Shift
 */
function observeCLS(): void {
  if (!('PerformanceObserver' in window)) return;

  let clsValue = 0;
  let sessionValue = 0;
  let sessionEntries: PerformanceEntry[] = [];

  try {
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries() as (PerformanceEntry & { hadRecentInput: boolean; value: number })[]) {
        // Only count layout shifts without recent user input
        if (!entry.hadRecentInput) {
          const firstSessionEntry = sessionEntries[0] as PerformanceEntry | undefined;
          const lastSessionEntry = sessionEntries[sessionEntries.length - 1] as PerformanceEntry | undefined;

          // If entry is within 1s of last entry and 5s of first, add to session
          if (
            sessionValue &&
            entry.startTime - (lastSessionEntry?.startTime ?? 0) < 1000 &&
            entry.startTime - (firstSessionEntry?.startTime ?? 0) < 5000
          ) {
            sessionValue += entry.value;
            sessionEntries.push(entry);
          } else {
            sessionValue = entry.value;
            sessionEntries = [entry];
          }

          // Update CLS if this session is larger
          if (sessionValue > clsValue) {
            clsValue = sessionValue;
            reportMetric('cls', clsValue);
          }
        }
      }
    });

    observer.observe({ type: 'layout-shift', buffered: true });
  } catch (e) {
    // CLS not supported
  }
}

/**
 * Observe First Contentful Paint
 */
function observeFCP(): void {
  if (!('PerformanceObserver' in window)) return;

  try {
    const observer = new PerformanceObserver((list) => {
      const entries = list.getEntries();
      const fcpEntry = entries.find((e) => e.name === 'first-contentful-paint');
      if (fcpEntry) {
        reportMetric('fcp', fcpEntry.startTime);
        observer.disconnect();
      }
    });

    observer.observe({ type: 'paint', buffered: true });
  } catch (e) {
    // FCP not supported
  }
}

/**
 * Mark a custom performance metric
 */
export function markPerformance(name: string): void {
  if (typeof performance !== 'undefined') {
    performance.mark(name);
  }
}

/**
 * Measure time between two marks
 */
export function measurePerformance(name: string, startMark: string, endMark?: string): number | null {
  if (typeof performance === 'undefined') return null;

  try {
    if (endMark) {
      performance.measure(name, startMark, endMark);
    } else {
      performance.measure(name, startMark);
    }

    const measures = performance.getEntriesByName(name, 'measure');
    const lastMeasure = measures[measures.length - 1];
    return lastMeasure?.duration ?? null;
  } catch (e) {
    return null;
  }
}

/**
 * Report custom timing metric
 */
export function reportCustomMetric(name: keyof PerformanceMetrics, value: number): void {
  reportMetric(name, value);
}

/**
 * Get all collected metrics
 */
export function getMetrics(): PerformanceMetrics {
  return { ...collectedMetrics };
}

/**
 * Log metrics to console (for debugging)
 */
export function logMetrics(): void {
  console.log('[Performance Metrics]', collectedMetrics);
}

/**
 * Check if performance is acceptable based on thresholds
 */
export function checkPerformance(): {
  score: 'good' | 'needs-improvement' | 'poor';
  details: Record<string, 'good' | 'needs-improvement' | 'poor'>;
} {
  const details: Record<string, 'good' | 'needs-improvement' | 'poor'> = {};
  let poorCount = 0;
  let needsImprovementCount = 0;

  // LCP thresholds: good < 2.5s, poor > 4s
  if (collectedMetrics.lcp !== undefined) {
    if (collectedMetrics.lcp < 2500) {
      details.lcp = 'good';
    } else if (collectedMetrics.lcp < 4000) {
      details.lcp = 'needs-improvement';
      needsImprovementCount++;
    } else {
      details.lcp = 'poor';
      poorCount++;
    }
  }

  // FID thresholds: good < 100ms, poor > 300ms
  if (collectedMetrics.fid !== undefined) {
    if (collectedMetrics.fid < 100) {
      details.fid = 'good';
    } else if (collectedMetrics.fid < 300) {
      details.fid = 'needs-improvement';
      needsImprovementCount++;
    } else {
      details.fid = 'poor';
      poorCount++;
    }
  }

  // CLS thresholds: good < 0.1, poor > 0.25
  if (collectedMetrics.cls !== undefined) {
    if (collectedMetrics.cls < 0.1) {
      details.cls = 'good';
    } else if (collectedMetrics.cls < 0.25) {
      details.cls = 'needs-improvement';
      needsImprovementCount++;
    } else {
      details.cls = 'poor';
      poorCount++;
    }
  }

  // FCP thresholds: good < 1.8s, poor > 3s
  if (collectedMetrics.fcp !== undefined) {
    if (collectedMetrics.fcp < 1800) {
      details.fcp = 'good';
    } else if (collectedMetrics.fcp < 3000) {
      details.fcp = 'needs-improvement';
      needsImprovementCount++;
    } else {
      details.fcp = 'poor';
      poorCount++;
    }
  }

  let score: 'good' | 'needs-improvement' | 'poor';
  if (poorCount > 0) {
    score = 'poor';
  } else if (needsImprovementCount > 0) {
    score = 'needs-improvement';
  } else {
    score = 'good';
  }

  return { score, details };
}
