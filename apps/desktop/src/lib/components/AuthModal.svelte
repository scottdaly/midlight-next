<script lang="ts">
  import { auth } from '@midlight/stores';
  import { authClient } from '$lib/auth';

  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();

  type Mode = 'login' | 'signup' | 'forgot-password';
  let mode = $state<Mode>('login');
  let email = $state('');
  let password = $state('');
  let displayName = $state('');
  let isLoading = $state(false);
  let error = $state<string | null>(null);
  let forgotPasswordSent = $state(false);

  function resetForm() {
    email = '';
    password = '';
    displayName = '';
    error = null;
    isLoading = false;
    forgotPasswordSent = false;
  }

  function switchMode() {
    mode = mode === 'login' ? 'signup' : 'login';
    error = null;
    forgotPasswordSent = false;
  }

  function goToForgotPassword() {
    mode = 'forgot-password';
    error = null;
    forgotPasswordSent = false;
  }

  function backToLogin() {
    mode = 'login';
    error = null;
    forgotPasswordSent = false;
  }

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    error = null;
    isLoading = true;

    try {
      if (mode === 'login') {
        await authClient.login(email, password);
        resetForm();
        onClose();
      } else if (mode === 'signup') {
        await authClient.signup(email, password, displayName || undefined);
        resetForm();
        onClose();
      } else if (mode === 'forgot-password') {
        await authClient.forgotPassword(email);
        forgotPasswordSent = true;
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      isLoading = false;
    }
  }

  async function handleGoogleLogin() {
    error = null;
    isLoading = true;

    try {
      await authClient.loginWithGoogle();
      // OAuth callback will close the modal via event
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      isLoading = false;
    }
  }

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

  // Close modal when authenticated
  $effect(() => {
    if ($auth.isAuthenticated && open) {
      resetForm();
      onClose();
    }
  });
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
      class="bg-card border border-border rounded-lg shadow-xl w-full max-w-md overflow-hidden"
      onclick={handleModalClick}
    >
      <!-- Header -->
      <div class="p-6 border-b border-border">
        <div class="flex justify-between items-center">
          <h2 class="text-xl font-semibold">
            {#if mode === 'login'}
              Welcome back
            {:else if mode === 'signup'}
              Create account
            {:else}
              Reset password
            {/if}
          </h2>
          <button
            onclick={onClose}
            aria-label="Close"
            class="p-1 hover:bg-accent rounded text-muted-foreground hover:text-foreground transition-colors"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M18 6 6 18"></path>
              <path d="M6 6 18 18"></path>
            </svg>
          </button>
        </div>
        <p class="text-sm text-muted-foreground mt-1">
          {#if mode === 'login'}
            Sign in to access AI features
          {:else if mode === 'signup'}
            Create an account to get started
          {:else}
            Enter your email to receive a password reset link
          {/if}
        </p>
      </div>

      <!-- Content -->
      <div class="p-6 space-y-4">
        <!-- Error message -->
        {#if error}
          <div class="p-3 bg-destructive/10 border border-destructive/20 rounded-md text-sm text-destructive">
            {error}
          </div>
        {/if}

        {#if mode === 'forgot-password'}
          <!-- Forgot Password Form -->
          {#if forgotPasswordSent}
            <!-- Success message -->
            <div class="text-center py-4">
              <div class="w-12 h-12 rounded-full bg-green-500/10 flex items-center justify-center mx-auto mb-4">
                <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-green-500">
                  <polyline points="20 6 9 17 4 12"></polyline>
                </svg>
              </div>
              <h3 class="text-lg font-medium mb-2">Check your email</h3>
              <p class="text-sm text-muted-foreground mb-4">
                We've sent a password reset link to <strong>{email}</strong>. Click the link in the email to reset your password.
              </p>
              <button
                onclick={backToLogin}
                class="text-sm text-primary hover:underline"
              >
                Back to sign in
              </button>
            </div>
          {:else}
            <form onsubmit={handleSubmit} class="space-y-4">
              <div>
                <label for="email" class="block text-sm font-medium mb-1.5">
                  Email
                </label>
                <input
                  type="email"
                  id="email"
                  bind:value={email}
                  placeholder="you@example.com"
                  required
                  class="w-full px-3 py-2 text-sm bg-background border border-border rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                />
              </div>

              <button
                type="submit"
                disabled={isLoading}
                class="w-full px-4 py-2.5 bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed font-medium"
              >
                {#if isLoading}
                  <span class="flex items-center justify-center gap-2">
                    <svg class="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                      <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                      <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                    </svg>
                    Sending...
                  </span>
                {:else}
                  Send reset link
                {/if}
              </button>

              <button
                type="button"
                onclick={backToLogin}
                class="w-full text-sm text-muted-foreground hover:text-foreground transition-colors"
              >
                Back to sign in
              </button>
            </form>
          {/if}
        {:else}
          <!-- Login/Signup Form -->
          <!-- Google Sign In -->
          <button
            onclick={handleGoogleLogin}
            disabled={isLoading}
            class="w-full flex items-center justify-center gap-3 px-4 py-2.5 border border-border rounded-md hover:bg-accent transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24">
              <path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"/>
              <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"/>
              <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"/>
              <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"/>
            </svg>
            <span>Continue with Google</span>
          </button>

          <div class="relative">
            <div class="absolute inset-0 flex items-center">
              <div class="w-full border-t border-border"></div>
            </div>
            <div class="relative flex justify-center text-xs uppercase">
              <span class="bg-card px-2 text-muted-foreground">Or continue with email</span>
            </div>
          </div>

          <!-- Email/Password Form -->
          <form onsubmit={handleSubmit} class="space-y-4">
            {#if mode === 'signup'}
              <div>
                <label for="displayName" class="block text-sm font-medium mb-1.5">
                  Name
                </label>
                <input
                  type="text"
                  id="displayName"
                  bind:value={displayName}
                  placeholder="Your name"
                  class="w-full px-3 py-2 text-sm bg-background border border-border rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                />
              </div>
            {/if}

            <div>
              <label for="email" class="block text-sm font-medium mb-1.5">
                Email
              </label>
              <input
                type="email"
                id="email"
                bind:value={email}
                placeholder="you@example.com"
                required
                class="w-full px-3 py-2 text-sm bg-background border border-border rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
              />
            </div>

            <div>
              <div class="flex items-center justify-between mb-1.5">
                <label for="password" class="block text-sm font-medium">
                  Password
                </label>
                {#if mode === 'login'}
                  <button
                    type="button"
                    onclick={goToForgotPassword}
                    class="text-xs text-primary hover:underline"
                  >
                    Forgot password?
                  </button>
                {/if}
              </div>
              <input
                type="password"
                id="password"
                bind:value={password}
                placeholder={mode === 'signup' ? 'Create a password' : 'Enter your password'}
                required
                minlength={8}
                class="w-full px-3 py-2 text-sm bg-background border border-border rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
              />
              {#if mode === 'signup'}
                <p class="text-xs text-muted-foreground mt-1">
                  At least 8 characters with uppercase, lowercase, and number
                </p>
              {/if}
            </div>

            <button
              type="submit"
              disabled={isLoading}
              class="w-full px-4 py-2.5 bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed font-medium"
            >
              {#if isLoading}
                <span class="flex items-center justify-center gap-2">
                  <svg class="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                    <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                    <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                  </svg>
                  {mode === 'login' ? 'Signing in...' : 'Creating account...'}
                </span>
              {:else}
                {mode === 'login' ? 'Sign in' : 'Create account'}
              {/if}
            </button>
          </form>
        {/if}
      </div>

      <!-- Footer -->
      {#if mode !== 'forgot-password'}
        <div class="px-6 py-4 border-t border-border bg-muted/30">
          <p class="text-sm text-center text-muted-foreground">
            {mode === 'login' ? "Don't have an account?" : 'Already have an account?'}
            <button
              onclick={switchMode}
              class="text-primary hover:underline ml-1"
            >
              {mode === 'login' ? 'Sign up' : 'Sign in'}
            </button>
          </p>
        </div>
      {/if}
    </div>
  </div>
{/if}
