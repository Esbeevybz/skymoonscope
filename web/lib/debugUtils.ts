/**
 * Debug utilities for development-only logging.
 * All console output is gated behind NODE_ENV check to prevent
 * exposing sensitive data (auth entries, RPC responses) in production.
 */

const isDevelopment = typeof window !== 'undefined' && process.env.NODE_ENV === 'development';

export const debugLog = (...args: any[]) => {
  if (isDevelopment) {
    console.log(...args);
  }
};

export const debugError = (...args: any[]) => {
  if (isDevelopment) {
    console.error(...args);
  }
};

export const debugWarn = (...args: any[]) => {
  if (isDevelopment) {
    console.warn(...args);
  }
};

export const debugDebug = (...args: any[]) => {
  if (isDevelopment) {
    console.debug(...args);
  }
};
