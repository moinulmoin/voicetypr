import { render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { LicenseStatus } from '@/types';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('sonner', () => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
}));

import { invoke } from '@tauri-apps/api/core';
import { LicenseProvider, useLicense } from './LicenseContext';

const SECRET_KEY = 'SUPER-SECRET-LICENSE-KEY-XYZ';

const licensedStatus: LicenseStatus = {
  status: 'licensed',
  trial_days_left: undefined,
  license_type: 'lifetime',
  license_key: SECRET_KEY,
  expires_at: '2027-01-01T00:00:00Z',
};

function Probe() {
  useLicense();
  return null;
}

describe('LicenseContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('never logs the license key when checking status', async () => {
    vi.mocked(invoke).mockResolvedValue(licensedStatus);

    const debugSpy = vi.spyOn(console, 'debug').mockImplementation(() => {});

    render(
      <LicenseProvider>
        <Probe />
      </LicenseProvider>,
    );

    await waitFor(() => expect(invoke).toHaveBeenCalledWith('check_license_status'));

    // Give the awaited resolution + the post-invoke debug log a tick to flush.
    await waitFor(() =>
      expect(
        debugSpy.mock.calls.some((c) => JSON.stringify(c).includes('License status received')),
      ).toBe(true),
    );

    for (const call of debugSpy.mock.calls) {
      expect(JSON.stringify(call)).not.toContain(SECRET_KEY);
    }

    // Sanity: the non-secret status fields ARE still logged (proves the check
    // ran and that we redacted rather than silencing the log entirely).
    const receivedLog = debugSpy.mock.calls.find((c) =>
      JSON.stringify(c).includes('License status received'),
    );
    expect(receivedLog).toBeDefined();
    const serialized = JSON.stringify(receivedLog);
    expect(serialized).toContain('"status":"licensed"');
    expect(serialized).toContain('"license_type":"lifetime"');
  });
});
