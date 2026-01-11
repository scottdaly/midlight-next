// @midlight/stores/settings - Settings state management

import { writable } from 'svelte/store';

export type Theme =
  | 'light'
  | 'dark'
  | 'midnight'
  | 'sepia'
  | 'forest'
  | 'cyberpunk'
  | 'coffee'
  | 'system';

export type PageMode = 'normal' | 'paginated';

export interface SettingsState {
  isOpen: boolean;
  theme: Theme;
  pageMode: PageMode;
  fontSize: number;
  fontFamily: string;
  spellcheck: boolean;
  autoSave: boolean;
  autoSaveInterval: number;
  showLineNumbers: boolean;
  errorReportingEnabled: boolean;
  apiKey: string;

  // Storage Settings
  rootFolderLocation: string;

  // Context Settings
  autoUpdateProjectContext: boolean;
  askBeforeSavingContext: boolean;
  showContextUpdateNotifications: boolean;
  learnAboutMeAutomatically: boolean;
  includeGlobalContext: boolean;
}

const defaultSettings: SettingsState = {
  isOpen: false,
  theme: 'system',
  pageMode: 'normal',
  fontSize: 16,
  fontFamily: 'Merriweather',
  spellcheck: true,
  autoSave: true,
  autoSaveInterval: 3000,
  showLineNumbers: false,
  errorReportingEnabled: false, // Opt-in only for privacy
  apiKey: '',

  // Storage defaults
  rootFolderLocation: '', // Empty means use default (Documents/Midlight/)

  // Context defaults (per vision document)
  autoUpdateProjectContext: true,
  askBeforeSavingContext: false,
  showContextUpdateNotifications: false,
  learnAboutMeAutomatically: true,
  includeGlobalContext: true,
};

// Try to load persisted settings
function loadPersistedSettings(): Partial<SettingsState> {
  if (typeof localStorage === 'undefined') return {};
  try {
    const stored = localStorage.getItem('midlight-settings');
    return stored ? JSON.parse(stored) : {};
  } catch {
    return {};
  }
}

// Save settings to localStorage
function persistSettings(settings: SettingsState) {
  if (typeof localStorage === 'undefined') return;
  try {
    const { isOpen, ...toStore } = settings;
    localStorage.setItem('midlight-settings', JSON.stringify(toStore));
  } catch {
    // Ignore storage errors
  }
}

function createSettingsStore() {
  const initial = { ...defaultSettings, ...loadPersistedSettings() };
  const { subscribe, set, update } = writable<SettingsState>(initial);

  // Subscribe to persist changes
  subscribe((settings) => persistSettings(settings));

  return {
    subscribe,

    /**
     * Opens the settings modal
     */
    open() {
      update((s) => ({ ...s, isOpen: true }));
    },

    /**
     * Closes the settings modal
     */
    close() {
      update((s) => ({ ...s, isOpen: false }));
    },

    /**
     * Sets the theme
     */
    setTheme(theme: Theme) {
      update((s) => ({ ...s, theme }));
    },

    /**
     * Sets the page mode
     */
    setPageMode(pageMode: PageMode) {
      update((s) => ({ ...s, pageMode }));
    },

    /**
     * Sets the font size
     */
    setFontSize(fontSize: number) {
      update((s) => ({ ...s, fontSize }));
    },

    /**
     * Sets the font family
     */
    setFontFamily(fontFamily: string) {
      update((s) => ({ ...s, fontFamily }));
    },

    /**
     * Sets spellcheck enabled
     */
    setSpellcheck(spellcheck: boolean) {
      update((s) => ({ ...s, spellcheck }));
    },

    /**
     * Sets auto-save enabled
     */
    setAutoSave(autoSave: boolean) {
      update((s) => ({ ...s, autoSave }));
    },

    /**
     * Sets auto-save interval
     */
    setAutoSaveInterval(interval: number) {
      update((s) => ({ ...s, autoSaveInterval: interval }));
    },

    /**
     * Sets error reporting enabled
     */
    setErrorReportingEnabled(enabled: boolean) {
      update((s) => ({ ...s, errorReportingEnabled: enabled }));
    },

    /**
     * Sets the API key for AI features
     */
    setApiKey(apiKey: string) {
      update((s) => ({ ...s, apiKey }));
    },

    /**
     * Sets root folder location
     */
    setRootFolderLocation(location: string) {
      update((s) => ({ ...s, rootFolderLocation: location }));
    },

    /**
     * Sets auto-update project context
     */
    setAutoUpdateProjectContext(enabled: boolean) {
      update((s) => ({ ...s, autoUpdateProjectContext: enabled }));
    },

    /**
     * Sets ask before saving context
     */
    setAskBeforeSavingContext(enabled: boolean) {
      update((s) => ({ ...s, askBeforeSavingContext: enabled }));
    },

    /**
     * Sets show context update notifications
     */
    setShowContextUpdateNotifications(enabled: boolean) {
      update((s) => ({ ...s, showContextUpdateNotifications: enabled }));
    },

    /**
     * Sets learn about me automatically
     */
    setLearnAboutMeAutomatically(enabled: boolean) {
      update((s) => ({ ...s, learnAboutMeAutomatically: enabled }));
    },

    /**
     * Sets include global context
     */
    setIncludeGlobalContext(enabled: boolean) {
      update((s) => ({ ...s, includeGlobalContext: enabled }));
    },

    /**
     * Resets to default settings
     */
    resetToDefaults() {
      set(defaultSettings);
    },
  };
}

export const settings = createSettingsStore();
