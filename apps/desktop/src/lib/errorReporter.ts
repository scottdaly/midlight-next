// Error Reporter client - Tauri invoke wrappers for error reporting
// Anonymous error reporting with PII sanitization (opt-in only)

import { invoke } from '@tauri-apps/api/core';

// ============================================================================
// Types
// ============================================================================

export type ErrorCategory =
  | 'import'
  | 'export'
  | 'file_system'
  | 'editor'
  | 'llm'
  | 'auth'
  | 'recovery'
  | 'unknown';

export interface ErrorReporterStatus {
  enabled: boolean;
  reports_this_session: number;
}

// Transformed type for frontend use (camelCase)
export interface ErrorReporterInfo {
  enabled: boolean;
  reportsThisSession: number;
}

// ============================================================================
// Error Reporter Client
// ============================================================================

class ErrorReporterClient {
  /**
   * Enable or disable error reporting
   */
  async setEnabled(enabled: boolean): Promise<void> {
    await invoke('error_reporter_set_enabled', { enabled });
  }

  /**
   * Get error reporting status
   */
  async getStatus(): Promise<ErrorReporterInfo> {
    const status = await invoke<ErrorReporterStatus>('error_reporter_get_status');
    return {
      enabled: status.enabled,
      reportsThisSession: status.reports_this_session,
    };
  }

  /**
   * Report an error
   * @param category - Error category for grouping
   * @param errorType - Specific error type (e.g., 'yaml_parse', 'network_timeout')
   * @param message - Error message (will be sanitized server-side)
   * @param context - Additional context (will be sanitized server-side)
   */
  async report(
    category: ErrorCategory,
    errorType: string,
    message: string,
    context?: Record<string, string>
  ): Promise<void> {
    await invoke('error_reporter_report', {
      category,
      errorType,
      message,
      context: context ?? null,
    });
  }

  /**
   * Report an error from a caught exception
   */
  async reportError(
    category: ErrorCategory,
    error: unknown,
    context?: Record<string, string>
  ): Promise<void> {
    let errorType = 'unknown';
    let message = 'Unknown error';

    if (error instanceof Error) {
      errorType = error.name || 'Error';
      message = error.message;
    } else if (typeof error === 'string') {
      errorType = 'string_error';
      message = error;
    } else if (error && typeof error === 'object') {
      errorType = 'object_error';
      message = JSON.stringify(error);
    }

    await this.report(category, errorType, message, context);
  }
}

export const errorReporter = new ErrorReporterClient();

// ============================================================================
// Convenience functions for common error categories
// ============================================================================

export async function reportImportError(
  error: unknown,
  context?: Record<string, string>
): Promise<void> {
  await errorReporter.reportError('import', error, context);
}

export async function reportExportError(
  error: unknown,
  context?: Record<string, string>
): Promise<void> {
  await errorReporter.reportError('export', error, context);
}

export async function reportEditorError(
  error: unknown,
  context?: Record<string, string>
): Promise<void> {
  await errorReporter.reportError('editor', error, context);
}

export async function reportLLMError(
  error: unknown,
  context?: Record<string, string>
): Promise<void> {
  await errorReporter.reportError('llm', error, context);
}

export async function reportAuthError(
  error: unknown,
  context?: Record<string, string>
): Promise<void> {
  await errorReporter.reportError('auth', error, context);
}
