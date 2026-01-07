// @midlight/stores/subscription - Subscription and quota state management

import { writable, derived } from 'svelte/store';

// ============================================================================
// Types
// ============================================================================

export interface SubscriptionStatus {
  tier: 'free' | 'premium' | 'pro';
  status: 'active' | 'past_due' | 'canceled' | 'trialing' | 'none';
  billingInterval?: string;
  currentPeriodEnd?: string;
}

export interface QuotaInfo {
  used: number;
  limit: number | null; // null = unlimited
  remaining: number | null;
}

export interface Price {
  id: string;
  productId: string;
  name: string;
  description?: string;
  amount: number;
  currency: string;
  interval: string;
  features?: string[];
  recommended?: boolean;
}

export interface SubscriptionState {
  status: SubscriptionStatus | null;
  quota: QuotaInfo | null;
  prices: Price[];
  isLoading: boolean;
  error: string | null;
}

// ============================================================================
// Initial State
// ============================================================================

const initialState: SubscriptionState = {
  status: null,
  quota: null,
  prices: [],
  isLoading: false,
  error: null,
};

// ============================================================================
// Store Creation
// ============================================================================

function createSubscriptionStore() {
  const { subscribe, set, update } = writable<SubscriptionState>(initialState);

  return {
    subscribe,

    /**
     * Set subscription status
     */
    setStatus(status: SubscriptionStatus | null) {
      update((s) => ({ ...s, status }));
    },

    /**
     * Set quota info
     */
    setQuota(quota: QuotaInfo | null) {
      update((s) => ({ ...s, quota }));
    },

    /**
     * Set available prices
     */
    setPrices(prices: Price[]) {
      update((s) => ({ ...s, prices }));
    },

    /**
     * Set loading state
     */
    setLoading(isLoading: boolean) {
      update((s) => ({ ...s, isLoading }));
    },

    /**
     * Set error state
     */
    setError(error: string | null) {
      update((s) => ({ ...s, error }));
    },

    /**
     * Clear error state
     */
    clearError() {
      update((s) => ({ ...s, error: null }));
    },

    /**
     * Reset store to initial state
     */
    reset() {
      set(initialState);
    },
  };
}

// ============================================================================
// Store Instance
// ============================================================================

export const subscription = createSubscriptionStore();

// ============================================================================
// Derived Stores
// ============================================================================

/**
 * Is the user on the free tier
 */
export const isFreeTier = derived(subscription, ($subscription) => {
  return !$subscription.status || $subscription.status.tier === 'free';
});

/**
 * Percentage of quota used (0-100)
 */
export const quotaPercentUsed = derived(subscription, ($subscription) => {
  if (!$subscription.quota || $subscription.quota.limit === null) {
    return 0;
  }
  return Math.min(100, ($subscription.quota.used / $subscription.quota.limit) * 100);
});

/**
 * Is the quota exceeded
 */
export const isQuotaExceeded = derived(subscription, ($subscription) => {
  if (!$subscription.quota) return false;
  if ($subscription.quota.remaining === null) return false; // Unlimited
  return $subscription.quota.remaining <= 0;
});

/**
 * Should show quota warning (>= 75% used)
 */
export const showQuotaWarning = derived(quotaPercentUsed, ($percent) => {
  return $percent >= 75;
});

/**
 * Quota warning severity
 */
export const quotaWarningSeverity = derived(quotaPercentUsed, ($percent) => {
  if ($percent >= 90) return 'critical';
  if ($percent >= 75) return 'warning';
  return 'none';
});

/**
 * Formatted quota display (e.g., "42/100" or "unlimited")
 */
export const quotaDisplay = derived(subscription, ($subscription) => {
  if (!$subscription.quota) return 'Loading...';
  if ($subscription.quota.limit === null) return 'Unlimited';
  return `${$subscription.quota.used}/${$subscription.quota.limit}`;
});
