// Subscription Types for Midlight

import type { QuotaInfo } from '../llm/types.js';

// ============================================================================
// Subscription Status Types
// ============================================================================

export type SubscriptionTier = 'free' | 'premium' | 'pro';
export type SubscriptionStatus = 'active' | 'past_due' | 'canceled' | 'trialing' | 'none';

export interface SubscriptionInfo {
  tier: SubscriptionTier;
  status: SubscriptionStatus;
  periodStart?: string; // ISO timestamp
  periodEnd?: string; // ISO timestamp
  cancelAtPeriodEnd?: boolean;
  customerId?: string;
}

// ============================================================================
// Pricing Types
// ============================================================================

export interface PriceInfo {
  id: string;
  productId: string;
  name: string;
  description?: string;
  amount: number; // In cents
  currency: string;
  interval: 'month' | 'year';
  features?: string[];
  recommended?: boolean;
}

// ============================================================================
// Checkout & Portal Types
// ============================================================================

export interface CheckoutSession {
  url: string;
  sessionId: string;
}

export interface PortalSession {
  url: string;
}

// ============================================================================
// Subscription Client Interface
// ============================================================================

export interface SubscriptionClient {
  /**
   * Get current subscription status
   */
  getStatus(): Promise<SubscriptionInfo>;

  /**
   * Get current quota information
   */
  getQuota(): Promise<QuotaInfo>;

  /**
   * Get available subscription prices
   */
  getPrices(): Promise<PriceInfo[]>;

  /**
   * Create a Stripe checkout session for subscription
   * Returns URL to redirect user to
   */
  createCheckoutSession(priceId: string): Promise<CheckoutSession>;

  /**
   * Create a Stripe billing portal session
   * Returns URL to redirect user to
   */
  createPortalSession(): Promise<PortalSession>;
}

// ============================================================================
// Error Types
// ============================================================================

export class SubscriptionError extends Error {
  constructor(
    message: string,
    public code: SubscriptionErrorCode,
    public details?: Record<string, unknown>
  ) {
    super(message);
    this.name = 'SubscriptionError';
  }
}

export type SubscriptionErrorCode =
  | 'AUTH_REQUIRED'
  | 'AUTH_EXPIRED'
  | 'NOT_FOUND'
  | 'PAYMENT_FAILED'
  | 'INVALID_REQUEST'
  | 'STRIPE_ERROR'
  | 'NETWORK_ERROR'
  | 'UNKNOWN';
