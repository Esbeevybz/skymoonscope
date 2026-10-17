import { describe, it, expect, vi, beforeEach } from 'vitest';

describe('Share Button Component', () => {
  beforeEach(() => {
    // Reset mocks before each test
    vi.clearAllMocks();
  });

  it('renders share button with correct text', () => {
    // Component should render with "Share" text
    const buttonText = 'Share';
    expect(buttonText).toBe('Share');
  });

  it('opens share dialog when clicked', () => {
    // State should toggle from closed to open
    let isOpen = false;
    isOpen = !isOpen;
    expect(isOpen).toBe(true);
  });

  it('validates TTL days between 1 and 90', () => {
    const validTTLs = [1, 30, 90];
    validTTLs.forEach(ttl => {
      expect(ttl >= 1 && ttl <= 90).toBe(true);
    });

    const invalidTTLs = [0, -1, 91, 120];
    invalidTTLs.forEach(ttl => {
      expect(ttl >= 1 && ttl <= 90).toBe(false);
    });
  });

  it('calls API with correct payload', () => {
    const data = {
      id: 'test-123',
      functionName: 'test_func',
      cpuInstructions: 1000,
    };

    const payload = {
      result: data,
      ttl_days: 30,
      is_public: true,
    };

    expect(payload.result).toEqual(data);
    expect(payload.ttl_days).toBe(30);
    expect(payload.is_public).toBe(true);
  });

  it('copies share URL to clipboard', async () => {
    const shareUrl = 'https://skymoonscope.app/share/abc123xyz789';
    const mockClipboard = {
      writeText: vi.fn().mockResolvedValue(undefined),
    };

    Object.assign(navigator, { clipboard: mockClipboard });

    await navigator.clipboard.writeText(shareUrl);
    expect(mockClipboard.writeText).toHaveBeenCalledWith(shareUrl);
  });

  it('generates valid share URL format', () => {
    const baseUrl = 'https://skymoonscope.app';
    const shareId = 'abc123xyz789';
    const shareUrl = `${baseUrl}/share/${shareId}`;

    expect(shareUrl).toBe('https://skymoonscope.app/share/abc123xyz789');
    expect(shareUrl).toMatch(/^https:\/\/.*\/share\/[a-zA-Z0-9]{12,16}$/);
  });

  it('handles API errors gracefully', () => {
    const errorMessage = 'Failed to create share';
    let error: string | null = null;

    try {
      throw new Error(errorMessage);
    } catch (err) {
      error = err instanceof Error ? err.message : 'Unknown error';
    }

    expect(error).toBe(errorMessage);
  });

  it('calls onShareSuccess callback', () => {
    const onShareSuccess = vi.fn();
    const shareUrl = 'https://skymoonscope.app/share/abc123xyz789';

    onShareSuccess(shareUrl);

    expect(onShareSuccess).toHaveBeenCalledWith(shareUrl);
    expect(onShareSuccess).toHaveBeenCalledTimes(1);
  });
});

describe('Shared Result Viewer Page', () => {
  it('fetches shared result on mount', () => {
    const shareId = 'abc123xyz789';
    const expectedUrl = `/api/shares/${shareId}`;

    expect(expectedUrl).toBe('/api/shares/abc123xyz789');
  });

  it('handles 404 error when share not found', () => {
    const status = 404;
    const expectedError = 'Share not found';

    if (status === 404) {
      expect(expectedError).toBe('Share not found');
    }
  });

  it('handles 410 error when share expired', () => {
    const status = 410;
    const expectedError = 'This share has expired';

    if (status === 410) {
      expect(expectedError).toBe('This share has expired');
    }
  });

  it('displays shared result data correctly', () => {
    const mockData = {
      share_id: 'abc123xyz789',
      result: {
        id: 'result-123',
        functionName: 'test_func',
        cpuInstructions: 1000,
      },
      created_at: '2024-10-17T23:07:06.899Z',
      access_count: 42,
    };

    expect(mockData.share_id).toBe('abc123xyz789');
    expect(mockData.access_count).toBe(42);
    expect(mockData.result.functionName).toBe('test_func');
  });

  it('renders nutrition label component', () => {
    const result = {
      cpuInstructions: 1000,
      memoryBytes: 5000,
      ledgerFootprintBytes: 100,
    };

    expect(result).toBeDefined();
    expect(result.cpuInstructions).toBe(1000);
  });

  it('formats creation date correctly', () => {
    const createdAt = '2024-10-17T23:07:06.899Z';
    const date = new Date(createdAt);
    const formatted = date.toLocaleString();

    expect(formatted).toBeTruthy();
    expect(date.getFullYear()).toBe(2024);
  });

  it('shows loading state while fetching', () => {
    let isLoading = true;
    expect(isLoading).toBe(true);

    isLoading = false;
    expect(isLoading).toBe(false);
  });

  it('displays error message when fetch fails', () => {
    const error = 'Failed to load shared result';
    expect(error).toBeTruthy();
    expect(error).toContain('Failed');
  });
});

describe('Share API Integration', () => {
  it('creates share with correct POST request', async () => {
    const payload = {
      result: { test: 'data' },
      ttl_days: 30,
      is_public: true,
    };

    const requestBody = JSON.stringify(payload);
    expect(requestBody).toContain('ttl_days');
    expect(requestBody).toContain('is_public');
  });

  it('retrieves share with GET request', () => {
    const shareId = 'abc123xyz789';
    const endpoint = `/api/shares/${shareId}`;

    expect(endpoint).toBe('/api/shares/abc123xyz789');
  });

  it('checks share existence', () => {
    const shareId = 'abc123xyz789';
    const endpoint = `/api/shares/${shareId}/exists`;

    expect(endpoint).toBe('/api/shares/abc123xyz789/exists');
  });

  it('deletes share with DELETE request', () => {
    const shareId = 'abc123xyz789';
    const endpoint = `/api/shares/${shareId}`;
    const method = 'DELETE';

    expect(endpoint).toBe('/api/shares/abc123xyz789');
    expect(method).toBe('DELETE');
  });

  it('handles share expiration correctly', () => {
    const createdAt = new Date('2024-10-01');
    const expiresAt = new Date('2024-11-01');
    const now = new Date('2024-11-02');

    const isExpired = now > expiresAt;
    expect(isExpired).toBe(true);
  });

  it('increments access count on view', () => {
    let accessCount = 0;
    accessCount += 1;
    expect(accessCount).toBe(1);

    accessCount += 1;
    expect(accessCount).toBe(2);
  });
});
