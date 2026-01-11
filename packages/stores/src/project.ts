// @midlight/stores/project - Project state management

import { writable, derived, get } from 'svelte/store';
import type { ProjectConfig, ProjectStatus } from '@midlight/core/types';

export interface ProjectInfo {
  path: string;
  config: ProjectConfig;
}

export interface ProjectState {
  projects: ProjectInfo[];
  isScanning: boolean;
  error: string | null;
}

const initialState: ProjectState = {
  projects: [],
  isScanning: false,
  error: null,
};

// Project scanner function type (injected from platform layer)
export type ProjectScanner = (workspaceRoot: string) => Promise<ProjectInfo[]>;

function createProjectStore() {
  const { subscribe, set, update } = writable<ProjectState>(initialState);

  // Project scanner will be set based on platform (Tauri or Web)
  let projectScanner: ProjectScanner | null = null;

  return {
    subscribe,

    /**
     * Sets the project scanner function (Tauri or Web)
     */
    setProjectScanner(scanner: ProjectScanner) {
      projectScanner = scanner;
    },

    /**
     * Scans workspace for projects
     */
    async scanProjects(workspaceRoot: string) {
      if (!projectScanner) {
        update((s) => ({ ...s, error: 'Project scanner not initialized' }));
        return;
      }

      update((s) => ({ ...s, isScanning: true, error: null }));

      try {
        const projects = await projectScanner(workspaceRoot);
        update((s) => ({
          ...s,
          projects,
          isScanning: false,
        }));
      } catch (error) {
        update((s) => ({
          ...s,
          isScanning: false,
          error: error instanceof Error ? error.message : String(error),
        }));
      }
    },

    /**
     * Adds a project to the store (for when a project is created locally)
     */
    addProject(project: ProjectInfo) {
      update((s) => ({
        ...s,
        projects: [...s.projects, project],
      }));
    },

    /**
     * Updates a project in the store
     */
    updateProject(path: string, config: Partial<ProjectConfig>) {
      update((s) => ({
        ...s,
        projects: s.projects.map((p) =>
          p.path === path ? { ...p, config: { ...p.config, ...config } } : p
        ),
      }));
    },

    /**
     * Removes a project from the store
     */
    removeProject(path: string) {
      update((s) => ({
        ...s,
        projects: s.projects.filter((p) => p.path !== path),
      }));
    },

    /**
     * Checks if a path is a known project
     */
    isProject(path: string): boolean {
      const state = get({ subscribe });
      return state.projects.some((p) => p.path === path);
    },

    /**
     * Gets project config for a path
     */
    getProjectConfig(path: string): ProjectConfig | null {
      const state = get({ subscribe });
      const project = state.projects.find((p) => p.path === path);
      return project?.config ?? null;
    },

    /**
     * Gets all projects with a specific status
     */
    getProjectsByStatus(status: ProjectStatus): ProjectInfo[] {
      const state = get({ subscribe });
      return state.projects.filter((p) => p.config.status === status);
    },

    /**
     * Sets the status of a project
     * Updates the store immediately, caller is responsible for persisting to disk
     */
    setProjectStatus(path: string, status: ProjectStatus) {
      update((s) => ({
        ...s,
        projects: s.projects.map((p) =>
          p.path === path ? { ...p, config: { ...p.config, status } } : p
        ),
      }));
    },

    /**
     * Gets the status of a project by path
     */
    getProjectStatus(path: string): ProjectStatus | null {
      const state = get({ subscribe });
      const project = state.projects.find((p) => p.path === path);
      return project?.config.status ?? null;
    },

    /**
     * Clears all project data
     */
    clear() {
      set(initialState);
    },

    /**
     * Resets the store
     */
    reset() {
      set(initialState);
    },
  };
}

export const projectStore = createProjectStore();

// Derived stores for convenient access
export const projects = derived(projectStore, ($ps) => $ps.projects);

export const activeProjects = derived(projectStore, ($ps) =>
  $ps.projects.filter((p) => p.config.status === 'active')
);

export const pausedProjects = derived(projectStore, ($ps) =>
  $ps.projects.filter((p) => p.config.status === 'paused')
);

export const archivedProjects = derived(projectStore, ($ps) =>
  $ps.projects.filter((p) => p.config.status === 'archived')
);

export const projectCount = derived(projectStore, ($ps) => $ps.projects.length);

export const isProjectScanning = derived(projectStore, ($ps) => $ps.isScanning);

export const projectPaths = derived(
  projectStore,
  ($ps) => new Set($ps.projects.map((p) => p.path))
);

export const projectError = derived(projectStore, ($ps) => $ps.error);
