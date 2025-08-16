import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { TabContainer } from './TabContainer';

// Mock all tab components with simple test versions
vi.mock('./RecordingsTab', () => ({
  RecordingsTab: () => <div data-testid="recordings-tab">Recordings</div>
}));

vi.mock('./ModelsTab', () => ({
  ModelsTab: () => <div data-testid="models-tab">Models</div>
}));

vi.mock('./SettingsTab', () => ({
  SettingsTab: () => <div data-testid="settings-tab">Settings</div>
}));

vi.mock('./EnhancementsTab', () => ({
  EnhancementsTab: () => <div data-testid="enhancements-tab">Enhancements</div>
}));

vi.mock('./AdvancedTab', () => ({
  AdvancedTab: () => <div data-testid="advanced-tab">Advanced</div>
}));

vi.mock('./AccountTab', () => ({
  AccountTab: () => <div data-testid="account-tab">Account</div>
}));

describe('TabContainer', () => {
  it('renders correct tab based on activeSection', () => {
    const { rerender } = render(<TabContainer activeSection="recordings" />);
    expect(screen.getByTestId('recordings-tab')).toBeInTheDocument();
    
    rerender(<TabContainer activeSection="models" />);
    expect(screen.getByTestId('models-tab')).toBeInTheDocument();
    
    rerender(<TabContainer activeSection="general" />);
    expect(screen.getByTestId('settings-tab')).toBeInTheDocument();
    
    rerender(<TabContainer activeSection="enhancements" />);
    expect(screen.getByTestId('enhancements-tab')).toBeInTheDocument();
    
    rerender(<TabContainer activeSection="advanced" />);
    expect(screen.getByTestId('advanced-tab')).toBeInTheDocument();
    
    rerender(<TabContainer activeSection="account" />);
    expect(screen.getByTestId('account-tab')).toBeInTheDocument();
  });

  it('renders recordings tab for unknown sections', () => {
    render(<TabContainer activeSection="unknown" />);
    expect(screen.getByTestId('recordings-tab')).toBeInTheDocument();
  });
});