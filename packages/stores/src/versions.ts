// @midlight/stores/versions - Version history state management

import { writable, derived } from 'svelte/store';
import type { Checkpoint, TiptapDocument } from '@midlight/core/types';

export interface VersionsState {
  isOpen: boolean;
  versions: Checkpoint[];
  selectedVersionId: string | null;
  previewContent: TiptapDocument | null;
  compareMode: boolean;
  compareVersionId: string | null;
  compareContent: TiptapDocument | null;
  isLoading: boolean;
}

const initialState: VersionsState = {
  isOpen: false,
  versions: [],
  selectedVersionId: null,
  previewContent: null,
  compareMode: false,
  compareVersionId: null,
  compareContent: null,
  isLoading: false,
};

function createVersionsStore() {
  const { subscribe, set, update } = writable<VersionsState>(initialState);

  return {
    subscribe,

    /**
     * Opens the versions panel
     */
    open() {
      update((s) => ({ ...s, isOpen: true }));
    },

    /**
     * Closes the versions panel
     */
    close() {
      update((s) => ({
        ...s,
        isOpen: false,
        selectedVersionId: null,
        previewContent: null,
        compareMode: false,
        compareVersionId: null,
        compareContent: null,
      }));
    },

    /**
     * Sets the versions list
     */
    setVersions(versions: Checkpoint[]) {
      update((s) => ({ ...s, versions }));
    },

    /**
     * Selects a version for preview
     */
    selectVersion(id: string | null) {
      update((s) => ({ ...s, selectedVersionId: id }));
    },

    /**
     * Sets preview content
     */
    setPreviewContent(content: TiptapDocument | null) {
      update((s) => ({ ...s, previewContent: content }));
    },

    /**
     * Enables compare mode
     */
    enableCompareMode(versionId: string) {
      update((s) => ({
        ...s,
        compareMode: true,
        compareVersionId: versionId,
      }));
    },

    /**
     * Disables compare mode
     */
    disableCompareMode() {
      update((s) => ({
        ...s,
        compareMode: false,
        compareVersionId: null,
        compareContent: null,
      }));
    },

    /**
     * Sets compare content
     */
    setCompareContent(content: TiptapDocument | null) {
      update((s) => ({ ...s, compareContent: content }));
    },

    /**
     * Sets loading state
     */
    setIsLoading(isLoading: boolean) {
      update((s) => ({ ...s, isLoading }));
    },

    /**
     * Resets the store
     */
    reset() {
      set(initialState);
    },
  };
}

export const versions = createVersionsStore();

// Derived stores
export const selectedVersion = derived(versions, ($v) =>
  $v.versions.find((v) => v.id === $v.selectedVersionId)
);
