<script lang="ts">
  import {
    workflowStore,
    workflowPhase,
    activeWorkflow,
    currentStep,
    workflowAnswers,
    validationErrors,
    executionProgress,
    executionResult,
    workflowError,
    isFirstStep,
    isLastStep,
  } from '@midlight/stores';

  let textInput = $state('');
  let textareaInput = $state('');
  let numberInput = $state<number | undefined>(undefined);
  let selectInput = $state('');
  let multiselectInput = $state<string[]>([]);

  // Update local state when step changes
  $effect(() => {
    if ($currentStep) {
      const existingValue = $workflowAnswers[$currentStep.id];
      switch ($currentStep.type) {
        case 'text':
          textInput = (existingValue as string) || '';
          break;
        case 'textarea':
          textareaInput = (existingValue as string) || '';
          break;
        case 'number':
          numberInput = existingValue as number | undefined;
          break;
        case 'select':
          selectInput = (existingValue as string) || '';
          break;
        case 'multiselect':
          multiselectInput = (existingValue as string[]) || [];
          break;
      }
    }
  });

  function handleInputChange() {
    if (!$currentStep) return;

    switch ($currentStep.type) {
      case 'text':
        workflowStore.setAnswer($currentStep.id, textInput);
        break;
      case 'textarea':
        workflowStore.setAnswer($currentStep.id, textareaInput);
        break;
      case 'number':
        if (numberInput !== undefined) {
          workflowStore.setAnswer($currentStep.id, numberInput);
        }
        break;
      case 'select':
        workflowStore.setAnswer($currentStep.id, selectInput);
        break;
      case 'multiselect':
        workflowStore.setAnswer($currentStep.id, multiselectInput);
        break;
    }
  }

  function toggleMultiselect(option: string) {
    if (multiselectInput.includes(option)) {
      multiselectInput = multiselectInput.filter((o) => o !== option);
    } else {
      multiselectInput = [...multiselectInput, option];
    }
    handleInputChange();
  }

  function handleNext() {
    handleInputChange();
    workflowStore.nextStep();
  }

  function handleBack() {
    workflowStore.previousStep();
  }

  function handleCancel() {
    workflowStore.cancel();
  }

  function handleClose() {
    workflowStore.close();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      if ($currentStep?.type !== 'textarea') {
        e.preventDefault();
        handleNext();
      }
    }
    if (e.key === 'Escape') {
      handleCancel();
    }
  }

  function handleBackdropClick() {
    if ($workflowPhase === 'interview') {
      handleCancel();
    }
  }

  function handleModalClick(e: MouseEvent) {
    e.stopPropagation();
  }

  function getProgressPhaseLabel(phase: string): string {
    switch (phase) {
      case 'creating-project':
        return 'Creating project...';
      case 'creating-context':
        return 'Setting up context...';
      case 'creating-files':
        return 'Creating files...';
      case 'generating-content':
        return 'Generating content with AI...';
      case 'complete':
        return 'Complete!';
      default:
        return 'Processing...';
    }
  }
</script>

<svelte:window onkeydown={$workflowPhase === 'interview' ? handleKeyDown : undefined} />

{#if $workflowPhase === 'interview' || $workflowPhase === 'executing' || $workflowPhase === 'complete' || $workflowPhase === 'error'}
  <!-- Backdrop -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4"
    onclick={handleBackdropClick}
  >
    <!-- Modal -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="bg-card border border-border rounded-lg shadow-xl max-w-lg w-full max-h-[80vh] flex flex-col"
      onclick={handleModalClick}
    >
      {#if $workflowPhase === 'interview' && $activeWorkflow && $currentStep}
        <!-- Interview Phase -->
        <!-- Header -->
        <div class="flex items-center justify-between px-6 py-4 border-b border-border">
          <div class="flex items-center gap-3">
            <span class="text-2xl">{$activeWorkflow.icon}</span>
            <div>
              <h2 class="text-sm font-semibold">{$activeWorkflow.name}</h2>
              <p class="text-xs text-muted-foreground">
                Step {workflowStore.getCurrentVisibleStepNumber()} of {workflowStore.getVisibleStepCount()}
              </p>
            </div>
          </div>
          <button
            onclick={handleCancel}
            class="p-1.5 hover:bg-muted rounded text-muted-foreground hover:text-foreground transition-colors"
            aria-label="Cancel"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M18 6 6 18"/>
              <path d="M6 6 18 18"/>
            </svg>
          </button>
        </div>

        <!-- Content -->
        <div class="flex-1 overflow-y-auto p-6">
          <div class="space-y-4">
            <!-- Question -->
            <div>
              <label for="workflow-input" class="block text-sm font-medium mb-2">
                {$currentStep.question}
                {#if $currentStep.required}
                  <span class="text-destructive">*</span>
                {/if}
              </label>

              <!-- Input based on type -->
              {#if $currentStep.type === 'text'}
                <input
                  id="workflow-input"
                  type="text"
                  bind:value={textInput}
                  oninput={handleInputChange}
                  placeholder={$currentStep.placeholder}
                  class="w-full px-3 py-2 border border-border rounded-md bg-background focus:outline-none focus:ring-2 focus:ring-ring"
                />
              {:else if $currentStep.type === 'textarea'}
                <textarea
                  id="workflow-input"
                  bind:value={textareaInput}
                  oninput={handleInputChange}
                  placeholder={$currentStep.placeholder}
                  rows={4}
                  class="w-full px-3 py-2 border border-border rounded-md bg-background focus:outline-none focus:ring-2 focus:ring-ring resize-none"
                ></textarea>
              {:else if $currentStep.type === 'number'}
                <input
                  id="workflow-input"
                  type="number"
                  bind:value={numberInput}
                  oninput={handleInputChange}
                  placeholder={$currentStep.placeholder}
                  class="w-full px-3 py-2 border border-border rounded-md bg-background focus:outline-none focus:ring-2 focus:ring-ring"
                />
              {:else if $currentStep.type === 'select' && $currentStep.options}
                <div class="space-y-2">
                  {#each $currentStep.options as option}
                    <button
                      onclick={() => {
                        selectInput = option;
                        handleInputChange();
                      }}
                      class="w-full text-left px-4 py-3 border rounded-md transition-all {selectInput === option
                        ? 'border-primary bg-primary/10 text-primary'
                        : 'border-border hover:border-primary/50 hover:bg-muted/50'}"
                    >
                      {option}
                    </button>
                  {/each}
                </div>
              {:else if $currentStep.type === 'multiselect' && $currentStep.options}
                <div class="space-y-2">
                  {#each $currentStep.options as option}
                    <button
                      onclick={() => toggleMultiselect(option)}
                      class="w-full text-left px-4 py-3 border rounded-md transition-all flex items-center gap-3 {multiselectInput.includes(option)
                        ? 'border-primary bg-primary/10'
                        : 'border-border hover:border-primary/50 hover:bg-muted/50'}"
                    >
                      <div
                        class="w-5 h-5 border-2 rounded flex items-center justify-center transition-all {multiselectInput.includes(option)
                          ? 'border-primary bg-primary'
                          : 'border-muted-foreground'}"
                      >
                        {#if multiselectInput.includes(option)}
                          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
                            <polyline points="20 6 9 17 4 12"/>
                          </svg>
                        {/if}
                      </div>
                      <span class={multiselectInput.includes(option) ? 'text-primary' : ''}>
                        {option}
                      </span>
                    </button>
                  {/each}
                </div>
              {/if}

              <!-- Help text -->
              {#if $currentStep.helpText}
                <p class="text-xs text-muted-foreground mt-2">
                  {$currentStep.helpText}
                </p>
              {/if}

              <!-- Validation error -->
              {#if $validationErrors[$currentStep.id]}
                <p class="text-xs text-destructive mt-2">
                  {$validationErrors[$currentStep.id]}
                </p>
              {/if}
            </div>
          </div>
        </div>

        <!-- Footer -->
        <div class="flex items-center justify-between px-6 py-4 border-t border-border bg-muted/30">
          <button
            onclick={handleBack}
            class="px-4 py-2 text-sm border border-border rounded-md hover:bg-muted transition-colors"
          >
            {$isFirstStep ? 'Back to Workflows' : 'Previous'}
          </button>
          <button
            onclick={handleNext}
            class="px-4 py-2 text-sm bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors"
          >
            {$isLastStep ? 'Create Project' : 'Next'}
          </button>
        </div>

      {:else if $workflowPhase === 'executing' && $executionProgress}
        <!-- Executing Phase -->
        <div class="p-8 text-center">
          <div class="flex justify-center mb-6">
            <div class="relative">
              <svg class="animate-spin h-16 w-16 text-primary" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              <span class="absolute inset-0 flex items-center justify-center text-lg font-semibold">
                {$executionProgress.percentComplete}%
              </span>
            </div>
          </div>

          <h2 class="text-lg font-semibold mb-2">
            Creating Your Project
          </h2>
          <p class="text-sm text-muted-foreground mb-6">
            {getProgressPhaseLabel($executionProgress.phase)}
          </p>

          {#if $executionProgress.currentFile}
            <p class="text-xs text-muted-foreground">
              {$executionProgress.currentFile}
            </p>
          {/if}

          <!-- Progress bar -->
          <div class="mt-6 h-2 bg-muted rounded-full overflow-hidden">
            <div
              class="h-full bg-primary transition-all duration-300"
              style="width: {$executionProgress.percentComplete}%"
            ></div>
          </div>
        </div>

      {:else if $workflowPhase === 'complete' && $executionResult}
        <!-- Complete Phase -->
        <div class="p-8">
          <div class="flex justify-center mb-6">
            <div class="w-16 h-16 bg-green-500/10 rounded-full flex items-center justify-center">
              <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-green-500">
                <polyline points="20 6 9 17 4 12"/>
              </svg>
            </div>
          </div>

          <h2 class="text-lg font-semibold text-center mb-2">
            Project Created!
          </h2>
          <p class="text-sm text-muted-foreground text-center mb-6">
            Your project has been set up and is ready to use.
          </p>

          <!-- Created files list -->
          <div class="bg-muted/50 rounded-lg p-4 mb-6">
            <h3 class="text-sm font-medium mb-2">Created files:</h3>
            <ul class="text-sm text-muted-foreground space-y-1">
              {#each $executionResult.createdFiles as file}
                <li class="flex items-center gap-2">
                  <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/>
                    <polyline points="14 2 14 8 20 8"/>
                  </svg>
                  {file}
                </li>
              {/each}
            </ul>
          </div>

          {#if $executionResult.failedFiles.length > 0}
            <div class="bg-destructive/10 text-destructive rounded-lg p-4 mb-6">
              <h3 class="text-sm font-medium mb-2">Some files could not be created:</h3>
              <ul class="text-sm space-y-1">
                {#each $executionResult.failedFiles as failed}
                  <li>{failed.path}: {failed.error}</li>
                {/each}
              </ul>
            </div>
          {/if}

          <button
            onclick={handleClose}
            class="w-full px-4 py-2 text-sm bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors"
          >
            Open Project
          </button>
        </div>

      {:else if $workflowPhase === 'error'}
        <!-- Error Phase -->
        <div class="p-8">
          <div class="flex justify-center mb-6">
            <div class="w-16 h-16 bg-destructive/10 rounded-full flex items-center justify-center">
              <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-destructive">
                <circle cx="12" cy="12" r="10"/>
                <line x1="15" y1="9" x2="9" y2="15"/>
                <line x1="9" y1="9" x2="15" y2="15"/>
              </svg>
            </div>
          </div>

          <h2 class="text-lg font-semibold text-center mb-2">
            Something went wrong
          </h2>
          <p class="text-sm text-muted-foreground text-center mb-6">
            {$workflowError || 'An unexpected error occurred while creating your project.'}
          </p>

          <div class="flex gap-3">
            <button
              onclick={handleCancel}
              class="flex-1 px-4 py-2 text-sm border border-border rounded-md hover:bg-muted transition-colors"
            >
              Cancel
            </button>
            <button
              onclick={() => workflowStore.startExecution()}
              class="flex-1 px-4 py-2 text-sm bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors"
            >
              Try Again
            </button>
          </div>
        </div>
      {/if}
    </div>
  </div>
{/if}
