// @midlight/stores/pwa - PWA install prompt handling

import { writable, derived } from 'svelte/store';

interface BeforeInstallPromptEvent extends Event {
  prompt: () => Promise<void>;
  userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>;
}

export interface PWAState {
  // Whether the app can be installed (install prompt available)
  canInstall: boolean;

  // Whether the app is already installed (running as standalone)
  isInstalled: boolean;

  // Whether the install prompt is currently showing
  isPromptShowing: boolean;

  // Whether the user has dismissed the install prompt
  wasDismissed: boolean;

  // Platform detection
  platform: 'ios' | 'android' | 'desktop' | 'unknown';

  // Whether we're running in a standalone window
  isStandalone: boolean;
}

const defaultState: PWAState = {
  canInstall: false,
  isInstalled: false,
  isPromptShowing: false,
  wasDismissed: false,
  platform: 'unknown',
  isStandalone: false,
};

function createPWAStore() {
  const { subscribe, set, update } = writable<PWAState>(defaultState);

  // Store the deferred prompt
  let deferredPrompt: BeforeInstallPromptEvent | null = null;

  return {
    subscribe,

    /**
     * Initialize PWA detection
     * Should be called once when the app starts
     */
    init() {
      if (typeof window === 'undefined') return;

      // Detect platform
      const userAgent = navigator.userAgent.toLowerCase();
      let platform: PWAState['platform'] = 'unknown';

      if (/iphone|ipad|ipod/.test(userAgent)) {
        platform = 'ios';
      } else if (/android/.test(userAgent)) {
        platform = 'android';
      } else if (/win|mac|linux/.test(userAgent)) {
        platform = 'desktop';
      }

      // Check if running in standalone mode
      const isStandalone =
        window.matchMedia('(display-mode: standalone)').matches ||
        (window.navigator as Navigator & { standalone?: boolean }).standalone === true;

      update((s) => ({
        ...s,
        platform,
        isStandalone,
        isInstalled: isStandalone,
      }));

      // Listen for install prompt
      window.addEventListener('beforeinstallprompt', (e) => {
        e.preventDefault();
        deferredPrompt = e as BeforeInstallPromptEvent;
        update((s) => ({ ...s, canInstall: true }));
      });

      // Listen for successful install
      window.addEventListener('appinstalled', () => {
        deferredPrompt = null;
        update((s) => ({
          ...s,
          canInstall: false,
          isInstalled: true,
        }));
      });

      // Check if previously dismissed
      try {
        const dismissed = localStorage.getItem('pwa-install-dismissed');
        if (dismissed) {
          update((s) => ({ ...s, wasDismissed: true }));
        }
      } catch {
        // Ignore localStorage errors
      }
    },

    /**
     * Show the install prompt
     */
    async promptInstall(): Promise<boolean> {
      if (!deferredPrompt) return false;

      update((s) => ({ ...s, isPromptShowing: true }));

      try {
        await deferredPrompt.prompt();
        const { outcome } = await deferredPrompt.userChoice;

        if (outcome === 'accepted') {
          deferredPrompt = null;
          update((s) => ({
            ...s,
            canInstall: false,
            isInstalled: true,
            isPromptShowing: false,
          }));
          return true;
        } else {
          update((s) => ({
            ...s,
            isPromptShowing: false,
            wasDismissed: true,
          }));

          // Remember dismissal
          try {
            localStorage.setItem('pwa-install-dismissed', 'true');
          } catch {
            // Ignore localStorage errors
          }

          return false;
        }
      } catch {
        update((s) => ({ ...s, isPromptShowing: false }));
        return false;
      }
    },

    /**
     * Get iOS-specific install instructions
     */
    getIOSInstructions(): string {
      return 'Tap the Share button, then "Add to Home Screen"';
    },

    /**
     * Reset the dismissed state (useful for settings)
     */
    resetDismissed() {
      try {
        localStorage.removeItem('pwa-install-dismissed');
      } catch {
        // Ignore localStorage errors
      }
      update((s) => ({ ...s, wasDismissed: false }));
    },

    /**
     * Check if we should show an install banner
     */
    shouldShowBanner(): boolean {
      let state: PWAState = defaultState;
      subscribe((s) => (state = s))();

      return (
        state.canInstall &&
        !state.isInstalled &&
        !state.wasDismissed &&
        !state.isStandalone
      );
    },
  };
}

export const pwa = createPWAStore();

// Derived stores
export const canInstall = derived(pwa, ($pwa) => $pwa.canInstall && !$pwa.isInstalled);
export const isInstalled = derived(pwa, ($pwa) => $pwa.isInstalled || $pwa.isStandalone);
export const showInstallBanner = derived(
  pwa,
  ($pwa) => $pwa.canInstall && !$pwa.isInstalled && !$pwa.wasDismissed && !$pwa.isStandalone
);
