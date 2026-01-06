// Editor store for sharing TipTap editor instance across components
import { writable, derived } from 'svelte/store';

// Generic editor instance type - actual TipTap Editor type is used at the component level
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type EditorInstance = any;

// Store the editor instance
const editorInstance = writable<EditorInstance | null>(null);

// Derived store to check if editor is ready
const editorReady = derived(editorInstance, ($editor) => $editor !== null);

// Methods to interact with the store
export const editor = {
  subscribe: editorInstance.subscribe,
  set: (instance: EditorInstance | null) => editorInstance.set(instance),
  get: () => {
    let value: EditorInstance | null = null;
    editorInstance.subscribe((v) => (value = v))();
    return value;
  },
};

export { editorReady };
