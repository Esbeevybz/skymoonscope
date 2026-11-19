/**
 * Frontend API client for shareable links
 */

import { apiUrl } from './api';

export interface ShareResponse {
  share_id: string;
  share_url: string;
  expires_at: string;
}

export interface ViewShareResponse {
  share_id: string;
  result?: any;
  metadata?: any;
  created_at: string;
  access_count: number;
}

export interface ShareExistsResponse {
  exists: boolean;
  expires_at?: string;
}

export class ShareApi {
  /**
   * Create a shareable link for a profiling result
   */
  static async createShare(
    result: any,
    ttlDays: number = 30,
    isPublic: boolean = true
  ): Promise<ShareResponse> {
    const response = await fetch(apiUrl('/api/shares'), {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        result,
        ttl_days: ttlDays,
        is_public: isPublic,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to create share: ${error}`);
    }

    return response.json();
  }

  /**
   * Retrieve a shared profiling result
   */
  static async getShare(shareId: string): Promise<ViewShareResponse> {
    const response = await fetch(apiUrl(`/api/shares/${shareId}`));

    if (response.status === 404) {
      throw new Error('Share not found');
    }

    if (response.status === 410) {
      throw new Error('Share has expired');
    }

    if (!response.ok) {
      throw new Error(`Failed to retrieve share: ${response.statusText}`);
    }

    return response.json();
  }

  /**
   * Check if a share exists and is still valid
   */
  static async checkShareExists(shareId: string): Promise<ShareExistsResponse> {
    const response = await fetch(apiUrl(`/api/shares/${shareId}/exists`));

    if (!response.ok) {
      throw new Error('Failed to check share existence');
    }

    return response.json();
  }

  /**
   * Delete a shared result
   */
  static async deleteShare(shareId: string): Promise<void> {
    const response = await fetch(apiUrl(`/api/shares/${shareId}`), {
      method: 'DELETE',
    });

    if (!response.ok) {
      throw new Error('Failed to delete share');
    }
  }

  /**
   * Generate a shareable URL (without creating on backend)
   */
  static generateShareUrl(shareId: string): string {
    const baseUrl = typeof window !== 'undefined' ? window.location.origin : '';
    return `${baseUrl}/share/${shareId}`;
  }

  /**
   * Copy share URL to clipboard
   */
  static async copyToClipboard(url: string): Promise<void> {
    if (navigator.clipboard) {
      await navigator.clipboard.writeText(url);
    } else {
      // Fallback for older browsers
      const textarea = document.createElement('textarea');
      textarea.value = url;
      document.body.appendChild(textarea);
      textarea.select();
      document.execCommand('copy');
      document.body.removeChild(textarea);
    }
  }
}

export default ShareApi;
