// @midlight/stores/auth - Authentication state management

import { writable, derived } from 'svelte/store';

export interface User {
  id: string;
  email: string;
  displayName: string;
  avatarUrl?: string;
}

export interface Subscription {
  tier: 'free' | 'premium' | 'pro';
  status: 'active' | 'cancelled' | 'expired' | 'past_due';
  billingInterval?: 'monthly' | 'yearly';
  currentPeriodEnd?: string;
}

export interface Quota {
  used: number;
  limit: number;
  resetDate: string;
}

export interface AuthState {
  user: User | null;
  subscription: Subscription | null;
  quota: Quota | null;
  isAuthenticated: boolean;
  isInitializing: boolean;
  error: string | null;
}

const initialState: AuthState = {
  user: null,
  subscription: null,
  quota: null,
  isAuthenticated: false,
  isInitializing: true,
  error: null,
};

function createAuthStore() {
  const { subscribe, set, update } = writable<AuthState>(initialState);

  return {
    subscribe,

    /**
     * Sets the current user
     */
    setUser(user: User | null) {
      update((s) => ({
        ...s,
        user,
        isAuthenticated: user !== null,
        isInitializing: false,
      }));
    },

    /**
     * Sets the subscription info
     */
    setSubscription(subscription: Subscription | null) {
      update((s) => ({ ...s, subscription }));
    },

    /**
     * Sets the quota info
     */
    setQuota(quota: Quota | null) {
      update((s) => ({ ...s, quota }));
    },

    /**
     * Sets initializing state
     */
    setIsInitializing(isInitializing: boolean) {
      update((s) => ({ ...s, isInitializing }));
    },

    /**
     * Sets error state
     */
    setError(error: string | null) {
      update((s) => ({ ...s, error }));
    },

    /**
     * Logs out the user
     */
    logout() {
      set({
        ...initialState,
        isInitializing: false,
      });
    },

    /**
     * Resets the store
     */
    reset() {
      set(initialState);
    },
  };
}

export const auth = createAuthStore();

// Derived stores
export const isAuthenticated = derived(auth, ($auth) => $auth.isAuthenticated);

export const currentUser = derived(auth, ($auth) => $auth.user);
