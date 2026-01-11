<script lang="ts">
  import { settings, auth, subscription, isFreeTier, quotaDisplay, quotaPercentUsed, shortcutsByCategory, getDisplayKey } from '@midlight/stores';
  import type { Theme } from '@midlight/stores';
  import { authClient } from '$lib/auth';
  import { subscriptionClient } from '$lib/subscription';
  import { errorReporter } from '$lib/errorReporter';
  import ThemePreview from './ThemePreview.svelte';

  interface Props {
    open: boolean;
    onClose: () => void;
    onOpenAuthModal?: () => void;
    onOpenUpgradeModal?: () => void;
  }

  let { open, onClose, onOpenAuthModal, onOpenUpgradeModal }: Props = $props();
  let isLoggingOut = $state(false);
  let isOpeningPortal = $state(false);

  // Account management state
  let showEditProfile = $state(false);
  let editDisplayName = $state('');
  let editEmail = $state('');
  let currentPassword = $state('');
  let newPassword = $state('');
  let confirmPassword = $state('');
  let isSavingProfile = $state(false);
  let profileError = $state<string | null>(null);
  let profileSuccess = $state(false);

  function startEditProfile() {
    editDisplayName = $auth.user?.displayName || '';
    editEmail = $auth.user?.email || '';
    currentPassword = '';
    newPassword = '';
    confirmPassword = '';
    profileError = null;
    profileSuccess = false;
    showEditProfile = true;
  }

  function cancelEditProfile() {
    showEditProfile = false;
    profileError = null;
    profileSuccess = false;
  }

  async function handleSaveProfile() {
    profileError = null;
    profileSuccess = false;

    // Validate
    if (newPassword && newPassword !== confirmPassword) {
      profileError = 'New passwords do not match';
      return;
    }

    if (newPassword && newPassword.length < 8) {
      profileError = 'Password must be at least 8 characters';
      return;
    }

    if (newPassword && !currentPassword) {
      profileError = 'Current password is required to change password';
      return;
    }

    isSavingProfile = true;

    try {
      await authClient.updateProfile({
        displayName: editDisplayName !== $auth.user?.displayName ? editDisplayName : undefined,
        email: editEmail !== $auth.user?.email ? editEmail : undefined,
        currentPassword: currentPassword || undefined,
        newPassword: newPassword || undefined,
      });
      profileSuccess = true;
      // Clear password fields after successful save
      currentPassword = '';
      newPassword = '';
      confirmPassword = '';
    } catch (err) {
      profileError = err instanceof Error ? err.message : String(err);
    } finally {
      isSavingProfile = false;
    }
  }

  type Tab = 'appearance' | 'editor' | 'ai' | 'context' | 'general' | 'shortcuts';
  let activeTab = $state<Tab>('appearance');

  const tabs: { id: Tab; label: string }[] = [
    { id: 'appearance', label: 'Appearance' },
    { id: 'editor', label: 'Editor' },
    { id: 'ai', label: 'AI' },
    { id: 'context', label: 'Context' },
    { id: 'general', label: 'General' },
    { id: 'shortcuts', label: 'Shortcuts' },
  ];

  const categoryLabels: Record<string, string> = {
    file: 'File',
    editing: 'Editing',
    view: 'View',
    navigation: 'Navigation',
    ai: 'AI',
    other: 'Other',
  };

  const themes: Theme[] = ['light', 'dark', 'midnight', 'sepia', 'forest', 'cyberpunk', 'coffee', 'system'];

  const fontSizes = [12, 14, 16, 18, 20, 24];
  const fontFamilies = [
    { value: 'Merriweather', label: 'Merriweather' },
    { value: 'System', label: 'System' },
    { value: 'Georgia', label: 'Georgia' },
    { value: 'Arial', label: 'Arial' },
    { value: 'Times', label: 'Times' },
    { value: 'Courier', label: 'Courier' },
  ];

  const autoSaveIntervals = [
    { value: 1000, label: '1 second' },
    { value: 2000, label: '2 seconds' },
    { value: 3000, label: '3 seconds' },
    { value: 5000, label: '5 seconds' },
    { value: 10000, label: '10 seconds' },
  ];

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }

  function handleBackdropClick() {
    onClose();
  }

  function handleModalClick(e: MouseEvent) {
    e.stopPropagation();
  }
</script>

<svelte:window onkeydown={open ? handleKeyDown : undefined} />

{#if open}
  <!-- Backdrop -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 bg-black/50 z-50 flex items-center justify-center"
    onclick={handleBackdropClick}
  >
    <!-- Modal -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="bg-card border border-border rounded-lg shadow-xl max-w-4xl w-full h-[600px] flex overflow-hidden"
      onclick={handleModalClick}
    >
      <!-- Sidebar -->
      <div class="w-64 border-r border-border flex flex-col bg-background">
        <div class="p-4 border-b border-border">
          <h2 class="text-lg font-semibold">Settings</h2>
        </div>
        <nav class="flex-1 p-2">
          {#each tabs as tab}
            <button
              onclick={() => activeTab = tab.id}
              class="w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors
                     {activeTab === tab.id ? 'bg-accent text-accent-foreground' : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground'}"
            >
              {#if tab.id === 'appearance'}
                <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <circle cx="13.5" cy="6.5" r="0.5"></circle>
                  <circle cx="17.5" cy="10.5" r="0.5"></circle>
                  <circle cx="8.5" cy="7.5" r="0.5"></circle>
                  <circle cx="6.5" cy="12.5" r="0.5"></circle>
                  <path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.926 0 1.648-.746 1.648-1.688 0-.437-.18-.835-.437-1.125-.29-.289-.438-.652-.438-1.125a1.64 1.64 0 0 1 1.668-1.668h1.996c3.051 0 5.555-2.503 5.555-5.555C21.965 6.012 17.461 2 12 2z"></path>
                </svg>
              {:else if tab.id === 'editor'}
                <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M4 7V4h16v3"></path>
                  <path d="M9 20h6"></path>
                  <path d="M12 4v16"></path>
                </svg>
              {:else if tab.id === 'ai'}
                <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M12 8V4H8"></path>
                  <rect width="16" height="12" x="4" y="8" rx="2"></rect>
                  <path d="M2 14h2"></path>
                  <path d="M20 14h2"></path>
                  <path d="M15 13v2"></path>
                  <path d="M9 13v2"></path>
                </svg>
              {:else if tab.id === 'context'}
                <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <circle cx="12" cy="12" r="10"/>
                  <path d="M12 16v-4"/>
                  <path d="M12 8h.01"/>
                </svg>
              {:else if tab.id === 'general'}
                <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"></path>
                  <circle cx="12" cy="12" r="3"></circle>
                </svg>
              {:else if tab.id === 'shortcuts'}
                <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <rect width="20" height="16" x="2" y="4" rx="2" ry="2"></rect>
                  <path d="M6 8h.001"></path>
                  <path d="M10 8h.001"></path>
                  <path d="M14 8h.001"></path>
                  <path d="M18 8h.001"></path>
                  <path d="M8 12h.001"></path>
                  <path d="M12 12h.001"></path>
                  <path d="M16 12h.001"></path>
                  <path d="M7 16h10"></path>
                </svg>
              {/if}
              {tab.label}
            </button>
          {/each}
        </nav>
      </div>

      <!-- Content -->
      <div class="flex-1 flex flex-col min-w-0">
        <div class="p-6 border-b border-border flex justify-between items-center">
          <h3 class="text-lg font-medium">{tabs.find(t => t.id === activeTab)?.label}</h3>
          <button
            onclick={onClose}
            aria-label="Close settings"
            class="p-1 hover:bg-accent rounded text-muted-foreground hover:text-foreground transition-colors"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M18 6 6 18"></path>
              <path d="M6 6 18 18"></path>
            </svg>
          </button>
        </div>
        <div class="flex-1 overflow-y-auto p-6">
          {#if activeTab === 'appearance'}
            <!-- Theme Grid -->
            <div>
              <h4 class="text-sm font-medium mb-4">Theme</h4>
              <div class="grid grid-cols-3 gap-4">
                {#each themes as theme}
                  <ThemePreview
                    {theme}
                    selected={$settings.theme === theme}
                    onclick={() => settings.setTheme(theme)}
                  />
                {/each}
              </div>
            </div>
          {:else if activeTab === 'editor'}
            <!-- Editor Settings -->
            <div class="space-y-6">
              <!-- Font Size -->
              <div class="flex items-center justify-between py-3 border-b border-border">
                <div>
                  <div class="text-sm font-medium">Font Size</div>
                  <div class="text-xs text-muted-foreground">Editor text size in pixels</div>
                </div>
                <select
                  value={$settings.fontSize}
                  onchange={(e) => settings.setFontSize(Number(e.currentTarget.value))}
                  class="px-3 py-1.5 text-sm bg-background border border-border rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                >
                  {#each fontSizes as size}
                    <option value={size}>{size}px</option>
                  {/each}
                </select>
              </div>

              <!-- Font Family -->
              <div class="flex items-center justify-between py-3 border-b border-border">
                <div>
                  <div class="text-sm font-medium">Font Family</div>
                  <div class="text-xs text-muted-foreground">Primary font for the editor</div>
                </div>
                <select
                  value={$settings.fontFamily}
                  onchange={(e) => settings.setFontFamily(e.currentTarget.value)}
                  class="px-3 py-1.5 text-sm bg-background border border-border rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                >
                  {#each fontFamilies as font}
                    <option value={font.value}>{font.label}</option>
                  {/each}
                </select>
              </div>

              <!-- Spellcheck -->
              <div class="flex items-center justify-between py-3 border-b border-border">
                <div>
                  <div class="text-sm font-medium">Spellcheck</div>
                  <div class="text-xs text-muted-foreground">Enable browser spellcheck in the editor</div>
                </div>
                <button
                  onclick={() => settings.setSpellcheck(!$settings.spellcheck)}
                  role="switch"
                  aria-checked={$settings.spellcheck}
                  aria-label="Toggle spellcheck"
                  class="relative w-11 h-6 rounded-full transition-colors {$settings.spellcheck ? 'bg-primary' : 'bg-muted'}"
                >
                  <span
                    class="absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform {$settings.spellcheck ? 'translate-x-5' : 'translate-x-0'}"
                  ></span>
                </button>
              </div>
            </div>
          {:else if activeTab === 'ai'}
            <!-- AI Settings -->
            <div class="space-y-6">
              <!-- Account Section -->
              <div class="py-3 border-b border-border">
                <div class="text-sm font-medium mb-3">Account</div>
                {#if $auth.isAuthenticated && $auth.user}
                  <!-- Logged in state -->
                  <div class="flex items-center gap-3 mb-4">
                    {#if $auth.user.avatarUrl}
                      <img
                        src={$auth.user.avatarUrl}
                        alt="Avatar"
                        class="w-10 h-10 rounded-full"
                      />
                    {:else}
                      <div class="w-10 h-10 rounded-full bg-primary/10 flex items-center justify-center">
                        <span class="text-primary font-medium">
                          {$auth.user.displayName?.[0] || $auth.user.email[0].toUpperCase()}
                        </span>
                      </div>
                    {/if}
                    <div class="flex-1 min-w-0">
                      {#if $auth.user.displayName}
                        <div class="text-sm font-medium truncate">{$auth.user.displayName}</div>
                      {/if}
                      <div class="text-xs text-muted-foreground truncate">{$auth.user.email}</div>
                    </div>
                  </div>

                  <!-- Sign out button -->
                  <button
                    onclick={async () => {
                      isLoggingOut = true;
                      try {
                        await authClient.logout();
                      } finally {
                        isLoggingOut = false;
                      }
                    }}
                    disabled={isLoggingOut}
                    class="px-4 py-2 text-sm border border-border rounded-md hover:bg-accent transition-colors disabled:opacity-50"
                  >
                    {isLoggingOut ? 'Signing out...' : 'Sign out'}
                  </button>
                {:else}
                  <!-- Logged out state -->
                  <div class="text-sm text-muted-foreground mb-4">
                    Sign in to access AI features like chat and document assistance.
                  </div>
                  <button
                    onclick={() => {
                      onClose();
                      onOpenAuthModal?.();
                    }}
                    class="px-4 py-2 text-sm bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors"
                  >
                    Sign in
                  </button>
                {/if}
              </div>

              <!-- Subscription Section -->
              {#if $auth.isAuthenticated}
                <div class="py-3 border-b border-border">
                  <div class="text-sm font-medium mb-3">Subscription</div>

                  <!-- Plan info -->
                  <div class="flex items-center justify-between mb-4">
                    <div>
                      <div class="text-sm">
                        <span class="capitalize font-medium">{$subscription.status?.tier || 'Free'}</span> Plan
                      </div>
                      {#if $subscription.status?.status === 'active' && !$isFreeTier}
                        <div class="text-xs text-green-500">Active</div>
                      {/if}
                    </div>

                    {#if $isFreeTier}
                      <button
                        onclick={() => {
                          onClose();
                          onOpenUpgradeModal?.();
                        }}
                        class="px-4 py-2 text-sm bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors"
                      >
                        Upgrade
                      </button>
                    {:else}
                      <button
                        onclick={async () => {
                          isOpeningPortal = true;
                          try {
                            await subscriptionClient.openPortal();
                          } finally {
                            isOpeningPortal = false;
                          }
                        }}
                        disabled={isOpeningPortal}
                        class="px-4 py-2 text-sm border border-border rounded-md hover:bg-accent transition-colors disabled:opacity-50"
                      >
                        {isOpeningPortal ? 'Opening...' : 'Manage Subscription'}
                      </button>
                    {/if}
                  </div>

                  <!-- Quota display (free tier only) -->
                  {#if $isFreeTier && $subscription.quota}
                    <div class="bg-muted rounded-lg p-3">
                      <div class="flex items-center justify-between mb-2">
                        <span class="text-xs text-muted-foreground">Monthly Usage</span>
                        <span class="text-xs font-medium">{$quotaDisplay} messages</span>
                      </div>
                      <!-- Progress bar -->
                      <div class="h-2 bg-background rounded-full overflow-hidden">
                        <div
                          class="h-full transition-all {$quotaPercentUsed >= 90 ? 'bg-destructive' : $quotaPercentUsed >= 75 ? 'bg-amber-500' : 'bg-primary'}"
                          style="width: {Math.min(100, $quotaPercentUsed)}%"
                        ></div>
                      </div>
                      {#if $quotaPercentUsed >= 75}
                        <p class="text-xs text-muted-foreground mt-2">
                          {#if $quotaPercentUsed >= 100}
                            You've reached your limit. Upgrade for unlimited messages.
                          {:else}
                            Running low on messages. Consider upgrading for unlimited access.
                          {/if}
                        </p>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/if}

              <!-- Account Management Section -->
              {#if $auth.isAuthenticated}
                <div class="py-3 border-b border-border">
                  <div class="flex items-center justify-between mb-3">
                    <div class="text-sm font-medium">Account Settings</div>
                    {#if !showEditProfile}
                      <button
                        onclick={startEditProfile}
                        class="text-xs text-primary hover:underline"
                      >
                        Edit
                      </button>
                    {/if}
                  </div>

                  {#if showEditProfile}
                    <!-- Edit Profile Form -->
                    <div class="space-y-4">
                      {#if profileError}
                        <div class="p-2 bg-destructive/10 border border-destructive/20 rounded text-sm text-destructive">
                          {profileError}
                        </div>
                      {/if}

                      {#if profileSuccess}
                        <div class="p-2 bg-green-500/10 border border-green-500/20 rounded text-sm text-green-600 dark:text-green-400">
                          Profile updated successfully!
                        </div>
                      {/if}

                      <div>
                        <label for="editDisplayName" class="block text-xs font-medium text-muted-foreground mb-1">
                          Display Name
                        </label>
                        <input
                          type="text"
                          id="editDisplayName"
                          bind:value={editDisplayName}
                          class="w-full px-3 py-1.5 text-sm bg-background border border-border rounded-md focus:outline-none focus:ring-1 focus:ring-ring"
                        />
                      </div>

                      <div>
                        <label for="editEmail" class="block text-xs font-medium text-muted-foreground mb-1">
                          Email
                        </label>
                        <input
                          type="email"
                          id="editEmail"
                          bind:value={editEmail}
                          class="w-full px-3 py-1.5 text-sm bg-background border border-border rounded-md focus:outline-none focus:ring-1 focus:ring-ring"
                        />
                      </div>

                      <div class="pt-2 border-t border-border">
                        <p class="text-xs text-muted-foreground mb-3">
                          Change Password (leave blank to keep current)
                        </p>

                        <div class="space-y-3">
                          <div>
                            <label for="currentPassword" class="block text-xs font-medium text-muted-foreground mb-1">
                              Current Password
                            </label>
                            <input
                              type="password"
                              id="currentPassword"
                              bind:value={currentPassword}
                              placeholder="Required for password change"
                              class="w-full px-3 py-1.5 text-sm bg-background border border-border rounded-md focus:outline-none focus:ring-1 focus:ring-ring"
                            />
                          </div>

                          <div>
                            <label for="newPassword" class="block text-xs font-medium text-muted-foreground mb-1">
                              New Password
                            </label>
                            <input
                              type="password"
                              id="newPassword"
                              bind:value={newPassword}
                              placeholder="At least 8 characters"
                              class="w-full px-3 py-1.5 text-sm bg-background border border-border rounded-md focus:outline-none focus:ring-1 focus:ring-ring"
                            />
                          </div>

                          <div>
                            <label for="confirmPassword" class="block text-xs font-medium text-muted-foreground mb-1">
                              Confirm New Password
                            </label>
                            <input
                              type="password"
                              id="confirmPassword"
                              bind:value={confirmPassword}
                              placeholder="Repeat new password"
                              class="w-full px-3 py-1.5 text-sm bg-background border border-border rounded-md focus:outline-none focus:ring-1 focus:ring-ring"
                            />
                          </div>
                        </div>
                      </div>

                      <div class="flex gap-2 pt-2">
                        <button
                          onclick={handleSaveProfile}
                          disabled={isSavingProfile}
                          class="px-4 py-1.5 text-sm bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors disabled:opacity-50"
                        >
                          {isSavingProfile ? 'Saving...' : 'Save Changes'}
                        </button>
                        <button
                          onclick={cancelEditProfile}
                          disabled={isSavingProfile}
                          class="px-4 py-1.5 text-sm border border-border rounded-md hover:bg-accent transition-colors disabled:opacity-50"
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                  {:else}
                    <div class="text-sm text-muted-foreground">
                      <p>Email: {$auth.user?.email}</p>
                      {#if $auth.user?.displayName}
                        <p>Name: {$auth.user.displayName}</p>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/if}

              <div class="py-3">
                <p class="text-xs text-muted-foreground">
                  Your session is stored locally and used to authenticate AI requests through the Midlight service.
                </p>
              </div>
            </div>
          {:else if activeTab === 'context'}
            <!-- Context Settings -->
            <div class="space-y-6">
              <div class="py-2 px-3 bg-muted/50 rounded-md mb-6">
                <p class="text-xs text-muted-foreground">
                  Control how Midlight manages context for AI conversations. Context includes your personal information (me.midlight) and project-specific notes (context.midlight).
                </p>
              </div>

              <!-- Include Global Context -->
              <div class="flex items-center justify-between py-3 border-b border-border">
                <div>
                  <div class="text-sm font-medium">Include Global Context</div>
                  <div class="text-xs text-muted-foreground">Include me.midlight in all AI conversations</div>
                </div>
                <button
                  onclick={() => settings.setIncludeGlobalContext(!$settings.includeGlobalContext)}
                  role="switch"
                  aria-checked={$settings.includeGlobalContext}
                  aria-label="Toggle include global context"
                  class="relative w-11 h-6 rounded-full transition-colors {$settings.includeGlobalContext ? 'bg-primary' : 'bg-muted'}"
                >
                  <span
                    class="absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform {$settings.includeGlobalContext ? 'translate-x-5' : 'translate-x-0'}"
                  ></span>
                </button>
              </div>

              <!-- Auto-update Project Context -->
              <div class="flex items-center justify-between py-3 border-b border-border">
                <div>
                  <div class="text-sm font-medium">Auto-update Project Context</div>
                  <div class="text-xs text-muted-foreground">AI can update context.midlight with decisions and status changes</div>
                </div>
                <button
                  onclick={() => settings.setAutoUpdateProjectContext(!$settings.autoUpdateProjectContext)}
                  role="switch"
                  aria-checked={$settings.autoUpdateProjectContext}
                  aria-label="Toggle auto-update project context"
                  class="relative w-11 h-6 rounded-full transition-colors {$settings.autoUpdateProjectContext ? 'bg-primary' : 'bg-muted'}"
                >
                  <span
                    class="absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform {$settings.autoUpdateProjectContext ? 'translate-x-5' : 'translate-x-0'}"
                  ></span>
                </button>
              </div>

              <!-- Ask Before Saving Context -->
              <div class="flex items-center justify-between py-3 border-b border-border">
                <div>
                  <div class="text-sm font-medium">Ask Before Saving Context</div>
                  <div class="text-xs text-muted-foreground">Prompt before AI updates context documents</div>
                </div>
                <button
                  onclick={() => settings.setAskBeforeSavingContext(!$settings.askBeforeSavingContext)}
                  role="switch"
                  aria-checked={$settings.askBeforeSavingContext}
                  aria-label="Toggle ask before saving context"
                  class="relative w-11 h-6 rounded-full transition-colors {$settings.askBeforeSavingContext ? 'bg-primary' : 'bg-muted'}"
                >
                  <span
                    class="absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform {$settings.askBeforeSavingContext ? 'translate-x-5' : 'translate-x-0'}"
                  ></span>
                </button>
              </div>

              <!-- Show Context Update Notifications -->
              <div class="flex items-center justify-between py-3 border-b border-border">
                <div>
                  <div class="text-sm font-medium">Show Context Update Notifications</div>
                  <div class="text-xs text-muted-foreground">Display a notification when context is updated</div>
                </div>
                <button
                  onclick={() => settings.setShowContextUpdateNotifications(!$settings.showContextUpdateNotifications)}
                  role="switch"
                  aria-checked={$settings.showContextUpdateNotifications}
                  aria-label="Toggle show context update notifications"
                  class="relative w-11 h-6 rounded-full transition-colors {$settings.showContextUpdateNotifications ? 'bg-primary' : 'bg-muted'}"
                >
                  <span
                    class="absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform {$settings.showContextUpdateNotifications ? 'translate-x-5' : 'translate-x-0'}"
                  ></span>
                </button>
              </div>

              <!-- Learn About Me Automatically -->
              <div class="flex items-center justify-between py-3 border-b border-border">
                <div>
                  <div class="text-sm font-medium">Learn About Me Automatically</div>
                  <div class="text-xs text-muted-foreground">AI can update me.midlight based on conversations</div>
                </div>
                <button
                  onclick={() => settings.setLearnAboutMeAutomatically(!$settings.learnAboutMeAutomatically)}
                  role="switch"
                  aria-checked={$settings.learnAboutMeAutomatically}
                  aria-label="Toggle learn about me automatically"
                  class="relative w-11 h-6 rounded-full transition-colors {$settings.learnAboutMeAutomatically ? 'bg-primary' : 'bg-muted'}"
                >
                  <span
                    class="absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform {$settings.learnAboutMeAutomatically ? 'translate-x-5' : 'translate-x-0'}"
                  ></span>
                </button>
              </div>
            </div>
          {:else if activeTab === 'general'}
            <!-- General Settings -->
            <div class="space-y-6">
              <!-- Auto-save -->
              <div class="flex items-center justify-between py-3 border-b border-border">
                <div>
                  <div class="text-sm font-medium">Auto-save</div>
                  <div class="text-xs text-muted-foreground">Automatically save documents as you type</div>
                </div>
                <button
                  onclick={() => settings.setAutoSave(!$settings.autoSave)}
                  role="switch"
                  aria-checked={$settings.autoSave}
                  aria-label="Toggle auto-save"
                  class="relative w-11 h-6 rounded-full transition-colors {$settings.autoSave ? 'bg-primary' : 'bg-muted'}"
                >
                  <span
                    class="absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform {$settings.autoSave ? 'translate-x-5' : 'translate-x-0'}"
                  ></span>
                </button>
              </div>

              <!-- Auto-save Interval -->
              {#if $settings.autoSave}
                <div class="flex items-center justify-between py-3 border-b border-border">
                  <div>
                    <div class="text-sm font-medium">Auto-save Interval</div>
                    <div class="text-xs text-muted-foreground">How often to save changes</div>
                  </div>
                  <select
                    value={$settings.autoSaveInterval}
                    onchange={(e) => settings.setAutoSaveInterval(Number(e.currentTarget.value))}
                    class="px-3 py-1.5 text-sm bg-background border border-border rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                  >
                    {#each autoSaveIntervals as interval}
                      <option value={interval.value}>{interval.label}</option>
                    {/each}
                  </select>
                </div>
              {/if}

              <!-- Error Reporting -->
              <div class="flex items-center justify-between py-3 border-b border-border">
                <div>
                  <div class="text-sm font-medium">Error Reporting</div>
                  <div class="text-xs text-muted-foreground">Help improve Midlight by sending anonymous error reports</div>
                </div>
                <button
                  onclick={async () => {
                    const newValue = !$settings.errorReportingEnabled;
                    settings.setErrorReportingEnabled(newValue);
                    await errorReporter.setEnabled(newValue);
                  }}
                  role="switch"
                  aria-checked={$settings.errorReportingEnabled}
                  aria-label="Toggle error reporting"
                  class="relative w-11 h-6 rounded-full transition-colors {$settings.errorReportingEnabled ? 'bg-primary' : 'bg-muted'}"
                >
                  <span
                    class="absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform {$settings.errorReportingEnabled ? 'translate-x-5' : 'translate-x-0'}"
                  ></span>
                </button>
              </div>
              {#if $settings.errorReportingEnabled}
                <div class="py-2 px-3 bg-muted/50 rounded-md">
                  <p class="text-xs text-muted-foreground">
                    Error reports include: error type, app version, OS info. Reports never include file contents, names, or personal information.
                  </p>
                </div>
              {/if}
            </div>
          {:else if activeTab === 'shortcuts'}
            <!-- Keyboard Shortcuts Reference -->
            <div class="space-y-6">
              {#each Object.entries($shortcutsByCategory) as [category, categoryShortcuts]}
                {#if categoryShortcuts.length > 0}
                  <div>
                    <h4 class="text-sm font-medium mb-3 text-muted-foreground uppercase tracking-wide">
                      {categoryLabels[category] || category}
                    </h4>
                    <div class="space-y-2">
                      {#each categoryShortcuts as shortcut}
                        <div class="flex items-center justify-between py-2 px-3 bg-muted/30 rounded-md">
                          <span class="text-sm">{shortcut.description}</span>
                          <kbd class="px-2 py-1 text-xs font-mono bg-background border border-border rounded shadow-sm">
                            {getDisplayKey(shortcut.keys)}
                          </kbd>
                        </div>
                      {/each}
                    </div>
                  </div>
                {/if}
              {/each}
              {#if Object.values($shortcutsByCategory).every(s => s.length === 0)}
                <p class="text-sm text-muted-foreground text-center py-8">
                  No keyboard shortcuts registered
                </p>
              {/if}
            </div>
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}
