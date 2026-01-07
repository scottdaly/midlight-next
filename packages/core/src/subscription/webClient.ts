// WebSubscriptionClient - Fetch-based subscription client for web

import type {
  SubscriptionClient,
  SubscriptionInfo,
  PriceInfo,
  CheckoutSession,
  PortalSession,
} from './types';
import type { QuotaInfo } from '../llm/types.js';
import { SubscriptionError } from './types';

export interface WebSubscriptionClientConfig {
  baseUrl: string;
  getAuthToken: () => Promise<string | null>;
}

/**
 * Web-based subscription client using fetch for requests.
 * Communicates with the midlight.ai backend for subscription management.
 */
export class WebSubscriptionClient implements SubscriptionClient {
  private baseUrl: string;
  private getAuthToken: () => Promise<string | null>;

  constructor(config: WebSubscriptionClientConfig) {
    this.baseUrl = config.baseUrl.replace(/\/$/, ''); // Remove trailing slash
    this.getAuthToken = config.getAuthToken;
  }

  /**
   * Get authorization headers for API requests
   */
  private async getHeaders(): Promise<Headers> {
    const headers = new Headers({
      'Content-Type': 'application/json',
    });

    const token = await this.getAuthToken();
    if (token) {
      headers.set('Authorization', `Bearer ${token}`);
    }

    return headers;
  }

  /**
   * Handle API error responses
   */
  private async handleErrorResponse(response: Response): Promise<never> {
    let errorData: { code?: string; message?: string; details?: Record<string, unknown> } = {};

    try {
      errorData = await response.json();
    } catch {
      // Response body may not be JSON
    }

    const message = errorData.message || `HTTP ${response.status}: ${response.statusText}`;

    switch (response.status) {
      case 401:
        throw new SubscriptionError(message, 'AUTH_REQUIRED', errorData.details);
      case 403:
        throw new SubscriptionError(message, 'AUTH_EXPIRED', errorData.details);
      case 404:
        throw new SubscriptionError(message, 'NOT_FOUND', errorData.details);
      case 402:
        throw new SubscriptionError(message, 'PAYMENT_FAILED', errorData.details);
      case 400:
        throw new SubscriptionError(message, 'INVALID_REQUEST', errorData.details);
      default:
        if (response.status >= 500) {
          throw new SubscriptionError(message, 'STRIPE_ERROR', errorData.details);
        }
        throw new SubscriptionError(message, 'UNKNOWN', errorData.details);
    }
  }

  /**
   * Get current subscription status
   */
  async getStatus(): Promise<SubscriptionInfo> {
    const headers = await this.getHeaders();

    const response = await fetch(`${this.baseUrl}/api/subscription/status`, {
      method: 'GET',
      headers,
    });

    if (!response.ok) {
      // Return free tier if not authenticated
      if (response.status === 401) {
        return {
          tier: 'free',
          status: 'none',
        };
      }
      await this.handleErrorResponse(response);
    }

    return await response.json();
  }

  /**
   * Get current quota information
   */
  async getQuota(): Promise<QuotaInfo> {
    const headers = await this.getHeaders();

    const response = await fetch(`${this.baseUrl}/api/llm/quota`, {
      method: 'GET',
      headers,
    });

    if (!response.ok) {
      // Return default free quota if not authenticated
      if (response.status === 401) {
        return {
          tier: 'free',
          limit: 100,
          used: 0,
          remaining: 100,
        };
      }
      await this.handleErrorResponse(response);
    }

    return await response.json();
  }

  /**
   * Get available subscription prices
   */
  async getPrices(): Promise<PriceInfo[]> {
    const headers = await this.getHeaders();

    const response = await fetch(`${this.baseUrl}/api/subscription/prices`, {
      method: 'GET',
      headers,
    });

    if (!response.ok) {
      await this.handleErrorResponse(response);
    }

    const data = await response.json();
    return data.prices || data;
  }

  /**
   * Create a Stripe checkout session for subscription
   */
  async createCheckoutSession(priceId: string): Promise<CheckoutSession> {
    const headers = await this.getHeaders();

    const response = await fetch(`${this.baseUrl}/api/subscription/checkout`, {
      method: 'POST',
      headers,
      body: JSON.stringify({ priceId }),
    });

    if (!response.ok) {
      await this.handleErrorResponse(response);
    }

    return await response.json();
  }

  /**
   * Create a Stripe billing portal session
   */
  async createPortalSession(): Promise<PortalSession> {
    const headers = await this.getHeaders();

    const response = await fetch(`${this.baseUrl}/api/subscription/portal`, {
      method: 'POST',
      headers,
    });

    if (!response.ok) {
      await this.handleErrorResponse(response);
    }

    return await response.json();
  }
}

/**
 * Create a WebSubscriptionClient with default configuration
 */
export function createWebSubscriptionClient(
  baseUrl: string = 'https://midlight.ai',
  getAuthToken: () => Promise<string | null>
): WebSubscriptionClient {
  return new WebSubscriptionClient({
    baseUrl,
    getAuthToken,
  });
}
