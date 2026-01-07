// Subscription integration - Connects Tauri subscription commands to the subscription store

import { invoke } from '@tauri-apps/api/core';
import { subscription } from '@midlight/stores';
import type { SubscriptionStatus, QuotaInfo, Price } from '@midlight/stores';

// ============================================================================
// Response types from Rust backend
// ============================================================================

interface SubscriptionResponse {
  tier: string;
  status: string;
  billingInterval?: string;
  currentPeriodEnd?: string;
}

interface QuotaResponse {
  used: number;
  limit?: number | null;
  remaining?: number | null;
}

interface CheckoutResponse {
  url: string;
  sessionId?: string;
}

interface PortalResponse {
  url: string;
}

// ============================================================================
// Subscription Client
// ============================================================================

export const subscriptionClient = {
  /**
   * Initialize subscription data (fetch status and quota)
   */
  async init(): Promise<void> {
    await Promise.all([this.fetchStatus(), this.fetchQuota()]);
  },

  /**
   * Fetch current subscription status
   */
  async fetchStatus(): Promise<SubscriptionStatus> {
    subscription.setLoading(true);
    subscription.setError(null);

    try {
      const response = await invoke<SubscriptionResponse>('auth_get_subscription');

      const status: SubscriptionStatus = {
        tier: (response.tier as SubscriptionStatus['tier']) || 'free',
        status: (response.status as SubscriptionStatus['status']) || 'none',
        billingInterval: response.billingInterval,
        currentPeriodEnd: response.currentPeriodEnd,
      };

      subscription.setStatus(status);
      subscription.setLoading(false);
      return status;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      subscription.setError(errorMessage);
      subscription.setLoading(false);

      // Return default free status on error
      const defaultStatus: SubscriptionStatus = {
        tier: 'free',
        status: 'none',
      };
      subscription.setStatus(defaultStatus);
      return defaultStatus;
    }
  },

  /**
   * Fetch current quota information
   */
  async fetchQuota(): Promise<QuotaInfo> {
    subscription.setLoading(true);
    subscription.setError(null);

    try {
      const response = await invoke<QuotaResponse>('auth_get_quota');

      const quota: QuotaInfo = {
        used: response.used ?? 0,
        limit: response.limit ?? 100,
        remaining: response.remaining ?? (response.limit ? response.limit - response.used : 100),
      };

      subscription.setQuota(quota);
      subscription.setLoading(false);
      return quota;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      subscription.setError(errorMessage);
      subscription.setLoading(false);

      // Return default quota on error
      const defaultQuota: QuotaInfo = {
        used: 0,
        limit: 100,
        remaining: 100,
      };
      subscription.setQuota(defaultQuota);
      return defaultQuota;
    }
  },

  /**
   * Fetch available subscription prices
   */
  async fetchPrices(): Promise<Price[]> {
    subscription.setLoading(true);
    subscription.setError(null);

    try {
      const prices = await invoke<Price[]>('subscription_get_prices');
      subscription.setPrices(prices);
      subscription.setLoading(false);
      return prices;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      subscription.setError(errorMessage);
      subscription.setLoading(false);
      return [];
    }
  },

  /**
   * Create Stripe checkout session and open in browser
   * The Rust backend automatically opens the URL in the default browser
   */
  async openCheckout(priceId: string): Promise<string> {
    subscription.setLoading(true);
    subscription.setError(null);

    try {
      const response = await invoke<CheckoutResponse>('subscription_create_checkout', {
        priceId,
      });
      subscription.setLoading(false);
      return response.url;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      subscription.setError(errorMessage);
      subscription.setLoading(false);
      throw error;
    }
  },

  /**
   * Create Stripe billing portal session and open in browser
   * The Rust backend automatically opens the URL in the default browser
   */
  async openPortal(): Promise<string> {
    subscription.setLoading(true);
    subscription.setError(null);

    try {
      const response = await invoke<PortalResponse>('subscription_create_portal');
      subscription.setLoading(false);
      return response.url;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      subscription.setError(errorMessage);
      subscription.setLoading(false);
      throw error;
    }
  },

  /**
   * Refresh all subscription data
   */
  async refresh(): Promise<void> {
    await Promise.all([this.fetchStatus(), this.fetchQuota()]);
  },
};
