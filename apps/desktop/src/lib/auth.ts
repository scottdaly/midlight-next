// Auth integration - Connects Tauri auth commands to the auth store

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { auth } from '@midlight/stores';
import type { User, Subscription, Quota } from '@midlight/stores';

// ============================================================================
// Types matching Rust backend
// ============================================================================

interface AuthStateChangedEvent {
  state: string;
  user: User | null;
}

// ============================================================================
// Auth Client
// ============================================================================

export const authClient = {
  /**
   * Initialize auth - attempt silent refresh from stored session
   */
  async init(): Promise<void> {
    try {
      const state = await invoke<string>('auth_init');

      if (state === 'authenticated') {
        const user = await invoke<User | null>('auth_get_user');
        auth.setUser(user);

        // Fetch subscription and quota in background
        this.fetchSubscription().catch(console.error);
        this.fetchQuota().catch(console.error);
      } else {
        auth.setUser(null);
      }
    } catch (error) {
      console.error('Auth init failed:', error);
      auth.setUser(null);
    }
  },

  /**
   * Login with email and password
   */
  async login(email: string, password: string): Promise<User> {
    auth.setError(null);

    try {
      const user = await invoke<User>('auth_login', { email, password });
      auth.setUser(user);

      // Fetch subscription and quota in background
      this.fetchSubscription().catch(console.error);
      this.fetchQuota().catch(console.error);

      return user;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      auth.setError(message);
      throw error;
    }
  },

  /**
   * Sign up with email and password
   */
  async signup(
    email: string,
    password: string,
    displayName?: string
  ): Promise<User> {
    auth.setError(null);

    try {
      const user = await invoke<User>('auth_signup', {
        email,
        password,
        displayName,
      });
      auth.setUser(user);
      return user;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      auth.setError(message);
      throw error;
    }
  },

  /**
   * Login with Google OAuth
   */
  async loginWithGoogle(): Promise<void> {
    auth.setError(null);

    try {
      await invoke('auth_login_with_google');
      // OAuth callback will update the state via event
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      auth.setError(message);
      throw error;
    }
  },

  /**
   * Logout
   */
  async logout(): Promise<void> {
    try {
      await invoke('auth_logout');
    } catch (error) {
      console.error('Logout error:', error);
    }
    auth.logout();
  },

  /**
   * Fetch subscription info
   */
  async fetchSubscription(): Promise<void> {
    try {
      const subscription = await invoke<Subscription>('auth_get_subscription');
      auth.setSubscription(subscription);
    } catch (error) {
      console.error('Failed to fetch subscription:', error);
    }
  },

  /**
   * Fetch quota info
   */
  async fetchQuota(): Promise<void> {
    try {
      const quota = await invoke<Quota>('auth_get_quota');
      auth.setQuota(quota);
    } catch (error) {
      console.error('Failed to fetch quota:', error);
    }
  },

  /**
   * Get current access token for API requests
   */
  async getAccessToken(): Promise<string | null> {
    return await invoke<string | null>('auth_get_access_token');
  },

  /**
   * Check if user is authenticated
   */
  async isAuthenticated(): Promise<boolean> {
    return await invoke<boolean>('auth_is_authenticated');
  },
};

// ============================================================================
// Event Listeners
// ============================================================================

let unlistenAuthStateChanged: (() => void) | null = null;
let unlistenSessionExpired: (() => void) | null = null;

/**
 * Start listening for auth events from Rust
 */
export async function startAuthEventListeners(): Promise<void> {
  // Listen for auth state changes (e.g., from OAuth callback)
  unlistenAuthStateChanged = await listen<AuthStateChangedEvent>(
    'auth:state-changed',
    (event) => {
      const { state, user } = event.payload;
      if (state === 'authenticated' && user) {
        auth.setUser(user);
        // Fetch subscription and quota in background
        authClient.fetchSubscription().catch(console.error);
        authClient.fetchQuota().catch(console.error);
      } else {
        auth.logout();
      }
    }
  );

  // Listen for session expiration
  unlistenSessionExpired = await listen('auth:session-expired', () => {
    auth.logout();
    // The App component should show the auth modal when logged out
  });
}

/**
 * Stop listening for auth events
 */
export function stopAuthEventListeners(): void {
  if (unlistenAuthStateChanged) {
    unlistenAuthStateChanged();
    unlistenAuthStateChanged = null;
  }
  if (unlistenSessionExpired) {
    unlistenSessionExpired();
    unlistenSessionExpired = null;
  }
}
